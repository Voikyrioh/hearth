import { defineStore } from "pinia";
import { computed, ref, shallowRef, watch } from "vue";
import { cloneDraft, type FilterDraft, isUnfiltered, resolveFilter } from "@/audit/filters";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type AuditEntry,
  type AuditFilter,
  EMPTY_AUDIT_FILTER,
  failureOf,
  getLinkBridge,
  type LinkFailure,
  type Unsubscribe,
} from "@/link";
import { useLinkStore } from "./link";
import { useToastsStore } from "./toasts";

/** Lignes gardées en mémoire au plus : la liste reste fluide et la session de plusieurs jours bornée. */
export const AUDIT_WINDOW_MAX = 3000;
/** Entrées retenues derrière le bouton « N nouvelles entrées » ; au-delà, on relit à la demande. */
export const AUDIT_PENDING_MAX = 500;
/** Pages relues au plus pour rattraper un trou ; au-delà on repart de la tête du journal. */
export const AUDIT_CATCH_UP_MAX_PAGES = 10;
/** Calme attendu avant de relire la tête après des entrées reçues en direct. */
export const AUDIT_LIVE_DEBOUNCE_MS = 300;
/** Attente MAXIMALE : un flux continu ne repousse jamais la relecture au-delà. */
export const AUDIT_LIVE_MAX_WAIT_MS = 2000;
/** Vérification périodique de la tête (lien établi) : répare ce que le flux a perdu sans le dire. */
export const AUDIT_VERIFY_EVERY_MS = 30_000;
/** Attente d'un « Rechargement manuel » avant de dire que le serveur est toujours injoignable. */
export const AUDIT_RELOAD_WAIT_MS = 5000;
/** Intervalle minimal entre deux annonces de nouvelles entrées aux lecteurs d'écran. */
export const AUDIT_ANNOUNCE_MS = 2000;
const KNOWN_ACCOUNTS_MAX = 200;
const LIVE_BUFFER_MAX = 1000;

export type AuditStatus = "idle" | "loading" | "ready" | "error";

/** Insère `fresh` dans `list` (les deux de la plus récente à la plus ancienne), sans doublon. */
export function mergeDesc(list: readonly AuditEntry[], fresh: readonly AuditEntry[]): AuditEntry[] {
  const seen = new Set(list.map((entry) => entry.id));
  const added = fresh.filter((entry) => {
    if (seen.has(entry.id)) return false;
    seen.add(entry.id);
    return true;
  });
  if (added.length === 0) return list.slice();
  return [...list, ...added].sort((a, b) => b.id - a.id);
}

/** Copie simple d'un filtre (le pont ne reçoit jamais un objet réactif de Vue). */
function plain(filter: AuditFilter): AuditFilter {
  return {
    accounts: [...filter.accounts],
    kinds: [...filter.kinds],
    outcomes: [...filter.outcomes],
    fromS: filter.fromS,
    toS: filter.toS,
    text: filter.text,
  };
}

/**
 * Journal d'activité du serveur affiché (HRT-14). Une seule session à la fois : `open` à l'arrivée
 * sur la page, `close` au départ, rien n'est mémorisé d'une ouverture à l'autre.
 *
 * Invariants (le journal sert de preuve) :
 * - la liste est une suite CONTIGUË du journal filtré, de la plus récente à la plus ancienne, sans
 *   doublon (par `id`). Les identifiants croissent mais ne sont PAS garantis consécutifs (filtre,
 *   purge, entrées condensées) : la contiguïté ne se déduit donc jamais de l'identifiant d'une entrée
 *   reçue, elle est TENUE PAR LES LECTURES. `verifiedTop` est l'identifiant jusqu'où une lecture a
 *   prouvé qu'il ne manque rien ; une entrée du direct au-dessus de lui est un DOUTE (le flux perd
 *   des entrées sans le dire : retard du canal, de l'agent, coupure), et toute entrée du direct, tout
 *   avis de retard et un minuteur de vérification déclenchent une relecture de la tête jusqu'à
 *   `verifiedTop`, qui comble ce qui manque. Une relecture qui échoue SE VOIT (état d'erreur, bouton
 *   « Réessayer ») : jamais un résultat partiel présenté comme complet ;
 * - l'ordre ne change jamais sous les yeux : une entrée en direct s'ajoute en tête seulement si
 *   l'utilisateur y est, sinon elle attend derrière « N nouvelles entrées » (seul un trou comblé
 *   s'insère à sa place) ;
 * - un filtre actif s'applique AUSSI au direct, par l'agent lui-même (une relecture de la tête),
 *   jamais par une recopie locale de ses règles de recherche ;
 * - la mémoire est bornée (`AUDIT_WINDOW_MAX`, `AUDIT_PENDING_MAX`).
 */
export const useAuditStore = defineStore("audit", () => {
  const serverId = ref<string | null>(null);
  const applied = ref<AuditFilter>({ ...EMPTY_AUDIT_FILTER });
  const entries = shallowRef<AuditEntry[]>([]);
  const nextBefore = ref<number | null>(null);
  const status = ref<AuditStatus>("idle");
  const failure = ref<LinkFailure | null>(null);
  const loadingMore = ref(false);
  const loadMoreFailed = ref(false);
  const pending = shallowRef<AuditEntry[]>([]);
  const pendingOverflow = ref(false);
  const atTop = ref(true);
  const windowFull = ref(false);
  const knownAccounts = ref<string[]>([]);
  const exporting = ref(false);
  const reloading = ref(false);
  /** Compteur : la page remonte en haut de la liste quand il change. */
  const topSignal = ref(0);
  /** Dernière annonce pour les lecteurs d'écran (« 3 nouvelles entrées »). */
  const announcement = ref("");

  let unsubscribe: Unsubscribe | null = null;
  /** Change à chaque ouverture, fermeture ou nouveau chargement : écarte les réponses tardives. */
  let generation = 0;
  /** Change à chaque ouverture : un abonnement tardif d'une ouverture périmée se relâche. */
  let openSeq = 0;
  let liveBuffer: AuditEntry[] | null = null;
  let liveTimer: ReturnType<typeof setTimeout> | undefined;
  let liveFirstAt = 0;
  let verifyTimer: ReturnType<typeof setInterval> | undefined;
  let catching = false;
  let catchAgain = false;
  let resumed = false;
  let announceTimer: ReturnType<typeof setTimeout> | undefined;
  let announceCount = 0;
  /** Jusqu'où une lecture a PROUVÉ que rien ne manque (voir l'en-tête). */
  let verifiedTop = 0;
  /** Le brouillon appliqué : les périodes relatives se recalculent quand le jour change. */
  let appliedDraft: FilterDraft | null = null;

  const newCount = computed(() => pending.value.length);
  const hasMoreBelow = computed(() => nextBefore.value !== null);
  /** Un doute sur la tête : des entrées du direct que aucune lecture n'a encore confirmées. */
  const unverified = computed(() => (entries.value[0]?.id ?? 0) > verifiedTop);

  function bridge() {
    return getLinkBridge();
  }

  function rememberAccounts(list: readonly AuditEntry[]) {
    const known = new Set(knownAccounts.value);
    let changed = false;
    for (const entry of list) {
      if (entry.account && !known.has(entry.account) && known.size < KNOWN_ACCOUNTS_MAX) {
        known.add(entry.account);
        changed = true;
      }
    }
    if (changed) knownAccounts.value = [...known].sort((a, b) => a.localeCompare(b));
  }

  function top(): number {
    const first = entries.value[0];
    return first ? first.id : 0;
  }

  /** Plus petit id chargé quand il reste des entrées plus anciennes : plancher d'une insertion. */
  function floor(): number {
    const last = entries.value[entries.value.length - 1];
    return nextBefore.value !== null && last ? last.id : 0;
  }

  function setEntries(list: AuditEntry[]) {
    let next = list;
    if (next.length > AUDIT_WINDOW_MAX) {
      next = next.slice(0, AUDIT_WINDOW_MAX);
      const last = next[next.length - 1];
      // On a coupé le bas : la suite se relit à partir de la dernière entrée gardée.
      nextBefore.value = last ? last.id : nextBefore.value;
    }
    entries.value = next;
    windowFull.value = next.length >= AUDIT_WINDOW_MAX && nextBefore.value !== null;
    rememberAccounts(next);
  }

  /**
   * Ajoute des entrées : celles qui tombent DANS la liste (plus anciennes que sa tête : un trou
   * comblé) s'y insèrent à leur place ; les plus récentes vont en tête si l'utilisateur y est, sinon
   * derrière le bouton.
   */
  function ingest(list: readonly AuditEntry[]) {
    const known = new Set<number>();
    for (const entry of entries.value) known.add(entry.id);
    for (const entry of pending.value) known.add(entry.id);
    const lowest = floor();
    const fresh = list.filter((entry) => !known.has(entry.id) && entry.id > lowest);
    if (fresh.length === 0) return;
    rememberAccounts(fresh);
    const head = top();
    const inner = fresh.filter((entry) => entry.id < head);
    const outer = fresh.filter((entry) => entry.id >= head);
    if (inner.length > 0) setEntries(mergeDesc(entries.value, inner));
    if (outer.length === 0) return;
    if (atTop.value) {
      setEntries(mergeDesc(entries.value, outer));
      announce(outer.length);
      return;
    }
    const merged = mergeDesc(pending.value, outer);
    if (merged.length > AUDIT_PENDING_MAX) {
      // On garde les PLUS ANCIENNES (celles qui touchent la liste) : les plus récentes se relisent
      // au clic, la liste reste une suite sans trou.
      pending.value = merged.slice(merged.length - AUDIT_PENDING_MAX);
      pendingOverflow.value = true;
    } else {
      pending.value = merged;
    }
    announce(pending.value.length);
  }

  /** Annonce aux lecteurs d'écran (poli : n'interrompt rien), au plus une fois par `AUDIT_ANNOUNCE_MS`. */
  function announce(count: number) {
    announceCount = count;
    const text = () =>
      t(announceCount === 1 ? "audit.newEntriesOne" : "audit.newEntries", { n: announceCount });
    if (announceTimer !== undefined) return;
    announcement.value = text();
    announceTimer = setTimeout(() => {
      announceTimer = undefined;
      if (text() !== announcement.value) announcement.value = text();
    }, AUDIT_ANNOUNCE_MS);
  }

  function fail(error: unknown) {
    status.value = "error";
    failure.value = failureOf(error);
    if (failure.value === null) logUiError(error, "audit");
  }

  /**
   * Reprend la liste à zéro avec le filtre appliqué. `superseded` : une lecture plus récente a pris la
   * main (ce n'est PAS un échec : l'appelant ne restaure rien et ne notifie rien). `silent` : relecture interne (rattrapage), sans
   * spinner et sans remonter l'utilisateur en haut de la liste.
   */
  async function load(options: { silent?: boolean } = {}): Promise<"ok" | "failed" | "superseded"> {
    const id = serverId.value;
    if (id === null) return "failed";
    generation += 1;
    const mine = generation;
    if (!options.silent) status.value = "loading";
    failure.value = null;
    loadMoreFailed.value = false;
    liveBuffer = [];
    try {
      const page = await bridge().readAudit(id, plain(applied.value), null);
      if (mine !== generation) return "superseded";
      const buffered = liveBuffer ?? [];
      liveBuffer = null;
      nextBefore.value = page.nextBefore;
      pending.value = [];
      pendingOverflow.value = false;
      atTop.value = true;
      verifiedTop = page.events[0]?.id ?? 0;
      setEntries(mergeDesc(page.events, isUnfiltered(applied.value) ? buffered : []));
      status.value = "ready";
      if (!options.silent) topSignal.value += 1;
      // Ce qui est arrivé pendant la lecture n'est pas confirmé : une relecture le vérifie.
      if (buffered.length > 0) scheduleCatchUp();
      return "ok";
    } catch (error) {
      if (mine !== generation) return "superseded";
      liveBuffer = null;
      fail(error);
      return "failed";
    }
  }

  async function loadMore(): Promise<void> {
    const id = serverId.value;
    const cursor = nextBefore.value;
    if (id === null || cursor === null || loadingMore.value || windowFull.value) return;
    if (status.value === "loading") return;
    loadingMore.value = true;
    const mine = generation;
    try {
      const page = await bridge().readAudit(id, plain(applied.value), cursor);
      if (mine !== generation) return;
      const last = entries.value[entries.value.length - 1];
      const below = last ? last.id : Number.POSITIVE_INFINITY;
      // Seulement du plus ancien que ce qui est là : l'ordre au-dessus ne bouge pas.
      const older = page.events.filter((entry) => entry.id < below);
      nextBefore.value = page.nextBefore;
      setEntries(mergeDesc(entries.value, older));
      loadMoreFailed.value = false;
    } catch (error) {
      if (mine !== generation) return;
      // Le défilement s'arrête : on le DIT (bouton « Réessayer » sous le tableau).
      loadMoreFailed.value = true;
      if (failureOf(error) === null) logUiError(error, "audit");
    } finally {
      loadingMore.value = false;
    }
  }

  /** Les périodes relatives (« Aujourd'hui »…) ont-elles changé de jour depuis l'application ? */
  async function refreshPeriod(): Promise<boolean> {
    if (!appliedDraft) return false;
    const next = resolveFilter(appliedDraft, Date.now());
    if (!next || JSON.stringify(plain(next)) === JSON.stringify(plain(applied.value))) return false;
    applied.value = next;
    await load({ silent: true });
    return true;
  }

  /**
   * Relit la tête du journal filtré jusqu'à `verifiedTop` et ajoute ce qui manque : vérification de
   * la continuité, rattrapage après une coupure, et seule façon d'appliquer un filtre actif au
   * direct. Trop de pages à relire : on repart de la tête (en silence si l'utilisateur est en haut,
   * sinon derrière le bouton : on ne remplace pas ce qu'il lit). Un échec se voit.
   */
  async function catchUp(): Promise<void> {
    const id = serverId.value;
    if (id === null) return;
    if (catching) {
      catchAgain = true;
      return;
    }
    catching = true;
    const mine = generation;
    try {
      do {
        catchAgain = false;
        if (await refreshPeriod()) break;
        if (mine !== generation) return;
        if (entries.value.length === 0 && pending.value.length === 0) {
          await load({ silent: true });
          break;
        }
        const known = verifiedTop;
        const found: AuditEntry[] = [];
        let before: number | null = null;
        let reached = false;
        let pages = 0;
        let newest = 0;
        while (!reached && pages < AUDIT_CATCH_UP_MAX_PAGES) {
          const page = await bridge().readAudit(id, plain(applied.value), before);
          if (mine !== generation) return;
          pages += 1;
          found.push(...page.events);
          if (pages === 1) newest = page.events[0]?.id ?? 0;
          const lastId = page.events[page.events.length - 1]?.id ?? 0;
          reached = page.nextBefore === null || lastId <= known;
          before = page.nextBefore;
        }
        if (!reached) {
          if (atTop.value) await load({ silent: true });
          else {
            pending.value = [];
            pendingOverflow.value = true;
          }
          break;
        }
        ingest(found);
        verifiedTop = Math.max(verifiedTop, newest);
        status.value = "ready";
        failure.value = null;
        if (resumed) {
          resumed = false;
          useToastsStore().push({ kind: "success", message: t("audit.linkBack") });
        }
      } while (catchAgain);
    } catch (error) {
      if (mine === generation) fail(error);
    } finally {
      catching = false;
    }
  }

  /** Relecture de la tête après un délai de calme, sans jamais attendre plus de `AUDIT_LIVE_MAX_WAIT_MS`. */
  function scheduleCatchUp() {
    const now = Date.now();
    if (liveTimer === undefined) liveFirstAt = now;
    clearTimeout(liveTimer);
    const delay = Math.min(
      AUDIT_LIVE_DEBOUNCE_MS,
      Math.max(0, liveFirstAt + AUDIT_LIVE_MAX_WAIT_MS - now),
    );
    liveTimer = setTimeout(() => {
      liveTimer = undefined;
      void catchUp();
    }, delay);
  }

  /** Une entrée reçue en direct. */
  function onLive(entry: AuditEntry) {
    if (serverId.value === null) return;
    if (isUnfiltered(applied.value)) {
      if (liveBuffer) {
        if (liveBuffer.length < LIVE_BUFFER_MAX) liveBuffer.push(entry);
        return;
      }
      ingest([entry]);
    }
    // Filtre actif : l'entrée n'est qu'un signal, l'agent filtre. Sans filtre : elle s'affiche déjà,
    // mais rien ne prouve qu'elle suit la tête : une lecture le vérifie.
    scheduleCatchUp();
  }

  /** Le flux a perdu des entrées (avis de retard) : la tête est douteuse, on la relit. */
  function onGap() {
    if (serverId.value === null) return;
    scheduleCatchUp();
  }

  async function open(id: string): Promise<void> {
    await close();
    serverId.value = id;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    appliedDraft = null;
    openSeq += 1;
    const mine = openSeq;
    // Écoute posée d'abord, lecture ensuite : aucune entrée ne tombe entre les deux.
    try {
      const off = await bridge().onAudit(id, onLive, onGap);
      // Une ouverture plus récente a pris la place pendant l'attente : cet abonnement est à relâcher.
      if (openSeq !== mine) off();
      else unsubscribe = off;
    } catch (error) {
      logUiError(error, "audit");
    }
    if (openSeq !== mine) return;
    verifyTimer = setInterval(() => {
      // Seulement lien établi, et jamais en plus d'une lecture déjà en cours (une suffit).
      if (
        !catching &&
        serverId.value !== null &&
        useLinkStore().stateOf(serverId.value) === "connected"
      ) {
        void catchUp();
      }
    }, AUDIT_VERIFY_EVERY_MS);
    await load();
  }

  async function close(): Promise<void> {
    generation += 1;
    openSeq += 1;
    unsubscribe?.();
    unsubscribe = null;
    clearTimeout(liveTimer);
    liveTimer = undefined;
    clearInterval(verifyTimer);
    verifyTimer = undefined;
    clearTimeout(announceTimer);
    announceTimer = undefined;
    serverId.value = null;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    appliedDraft = null;
    entries.value = [];
    pending.value = [];
    pendingOverflow.value = false;
    nextBefore.value = null;
    status.value = "idle";
    failure.value = null;
    loadingMore.value = false;
    loadMoreFailed.value = false;
    windowFull.value = false;
    knownAccounts.value = [];
    atTop.value = true;
    announcement.value = "";
    liveBuffer = null;
    catching = false;
    catchAgain = false;
    resumed = false;
    verifiedTop = 0;
    reloading.value = false;
    exporting.value = false;
  }

  /**
   * l'application d'un filtre. `invalid` : la période est invalide, rien n'est lancé. `failed` : la
   * lecture a échoué, les filtres précédents et la liste affichée restent (BR-AUDIT-014).
   */
  async function apply(
    draft: FilterDraft,
    now: number = Date.now(),
  ): Promise<"applied" | "invalid" | "failed" | "superseded"> {
    const filter = resolveFilter(cloneDraft(draft), now);
    if (!filter) return "invalid";
    const previous = applied.value;
    const previousDraft = appliedDraft;
    applied.value = filter;
    appliedDraft = cloneDraft(draft);
    const outcome = await load();
    if (outcome === "ok") return "applied";
    // Dépassée par une application plus récente : son filtre est déjà posé, on n'y touche pas.
    if (outcome === "superseded") return "superseded";
    applied.value = previous;
    appliedDraft = previousDraft;
    return "failed";
  }

  /** « Effacer les filtres » : tout le journal ; `false` si la lecture échoue (l'état précédent reste). */
  async function clearFilters(): Promise<boolean> {
    const previous = applied.value;
    const previousDraft = appliedDraft;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    appliedDraft = null;
    const outcome = await load();
    if (outcome === "failed") {
      applied.value = previous;
      appliedDraft = previousDraft;
    }
    return outcome !== "failed";
  }

  /** « Réessayer » après un échec : relit ce qui manque (la liste affichée est gardée) ou tout. */
  async function retry(): Promise<void> {
    if (entries.value.length === 0) await load();
    else await catchUp();
  }

  /** Le lien revient : on rattrape ce qui a été manqué (BR-AUDIT-011). */
  function resume(): void {
    if (serverId.value === null) return;
    resumed = true;
    if (status.value === "error" && entries.value.length === 0) {
      void load().then(() => {
        if (status.value === "ready" && resumed) {
          resumed = false;
          useToastsStore().push({ kind: "success", message: t("audit.linkBack") });
        }
      });
      return;
    }
    void catchUp();
  }

  /** « N nouvelles entrées » : elles rejoignent la liste, qui remonte. */
  async function showPending(): Promise<void> {
    const overflow = pendingOverflow.value;
    const held = pending.value;
    pending.value = [];
    pendingOverflow.value = false;
    atTop.value = true;
    if (overflow) {
      // Ce qui s'est passé depuis dépasse ce qu'on retient : la tête se relit (l'utilisateur l'a demandé).
      await load();
      return;
    }
    setEntries(mergeDesc(entries.value, held));
    topSignal.value += 1;
  }

  function setAtTop(value: boolean): void {
    if (atTop.value === value) return;
    atTop.value = value;
    // Revenu en haut à la main : ce qui attendait rejoint la liste, le bouton disparaît.
    if (value && (pending.value.length > 0 || pendingOverflow.value)) void showPending();
  }

  /** Attend « Connecté » sans interroger en boucle : l'état du lien est réactif. */
  function untilConnected(id: string, ms: number): Promise<boolean> {
    const link = useLinkStore();
    if (link.stateOf(id) === "connected") return Promise.resolve(true);
    return new Promise((resolve) => {
      const stop = watch(
        () => link.stateOf(id),
        (state) => {
          if (state !== "connected") return;
          stop();
          clearTimeout(timer);
          resolve(true);
        },
      );
      const timer = setTimeout(() => {
        stop();
        resolve(false);
      }, ms);
    });
  }

  /** « Rechargement manuel » (BR-AUDIT-020) : une tentative de reconnexion, puis la relecture. */
  async function reloadManually(): Promise<void> {
    const id = serverId.value;
    if (id === null || reloading.value) return;
    reloading.value = true;
    const link = useLinkStore();
    try {
      await link.retryNow(id);
      const connected = await untilConnected(id, AUDIT_RELOAD_WAIT_MS);
      if (serverId.value !== id) return;
      if (!connected) {
        useToastsStore().push({ kind: "info", message: t("audit.stillUnreachable") });
      }
      // Connecté : `resume` (appelé par la page au changement d'état) fait la relecture.
    } catch (error) {
      logUiError(error, "audit");
      useToastsStore().push({ kind: "info", message: t("audit.stillUnreachable") });
    } finally {
      reloading.value = false;
    }
  }

  /** « Exporter » : le résultat filtré APPLIQUÉ, dans un fichier choisi par l'utilisateur. */
  async function exportCsv(): Promise<void> {
    const id = serverId.value;
    if (id === null || exporting.value) return;
    exporting.value = true;
    const toasts = useToastsStore();
    try {
      const result = await bridge().exportAudit(id, plain(applied.value));
      if (result.saved) {
        toasts.push({ kind: "success", message: t("audit.exportDone") });
        if (result.truncated) toasts.push({ kind: "info", message: t("audit.exportTruncated") });
      }
    } catch (error) {
      if (failureOf(error) === null) logUiError(error, "audit");
      toasts.push({ kind: "error", message: t("audit.exportFailed") });
    } finally {
      exporting.value = false;
    }
  }

  return {
    serverId,
    applied,
    entries,
    nextBefore,
    status,
    failure,
    loadingMore,
    loadMoreFailed,
    pending,
    pendingOverflow,
    newCount,
    atTop,
    windowFull,
    hasMoreBelow,
    unverified,
    knownAccounts,
    exporting,
    reloading,
    topSignal,
    announcement,
    open,
    close,
    apply,
    clearFilters,
    retry,
    loadMore,
    resume,
    showPending,
    setAtTop,
    reloadManually,
    exportCsv,
  };
});
