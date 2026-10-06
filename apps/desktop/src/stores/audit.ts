import { defineStore } from "pinia";
import { computed, ref, shallowRef } from "vue";
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
/** Regroupe les entrées reçues en rafale quand un filtre est actif (une relecture, pas une par entrée). */
export const AUDIT_LIVE_DEBOUNCE_MS = 300;
/** Attente d'un « Rechargement manuel » avant de dire que le serveur est toujours injoignable. */
export const AUDIT_RELOAD_WAIT_MS = 5000;
/** Intervalle minimal entre deux annonces de nouvelles entrées aux lecteurs d'écran. */
export const AUDIT_ANNOUNCE_MS = 2000;
const KNOWN_ACCOUNTS_MAX = 200;
const LIVE_BUFFER_MAX = 1000;

export type AuditStatus = "idle" | "loading" | "ready" | "error";

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

/**
 * Journal d'activité du serveur affiché (HRT-14). Une seule session à la fois : `open` à l'arrivée
 * sur la page, `close` au départ, rien n'est mémorisé d'une ouverture à l'autre (BR : les filtres ne
 * survivent pas à la fermeture).
 *
 * Invariants (le journal sert de preuve) :
 * - la liste est une suite CONTIGUË du journal filtré, de la plus récente à la plus ancienne, sans
 *   doublon (par `id`) : ni trou entre la page chargée et le flux, ni après une coupure (rattrapage) ;
 * - l'ordre ne change jamais sous les yeux : une entrée en direct s'ajoute en tête seulement si
 *   l'utilisateur y est, sinon elle attend derrière « N nouvelles entrées » ;
 * - un filtre actif s'applique AUSSI au direct, par l'agent lui-même (une relecture de la tête), jamais
 *   par une recopie locale de ses règles de recherche ;
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
  let generation = 0;
  let liveBuffer: AuditEntry[] | null = null;
  let liveTimer: ReturnType<typeof setTimeout> | undefined;
  let catching = false;
  let catchAgain = false;
  let resumed = false;
  let announceTimer: ReturnType<typeof setTimeout> | undefined;
  let announceCount = 0;

  const newCount = computed(() => pending.value.length);
  const hasMoreBelow = computed(() => nextBefore.value !== null);

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

  /** Ajoute des entrées plus récentes : en tête si on y est, sinon derrière le bouton. */
  function ingest(list: readonly AuditEntry[]) {
    const known = new Set<number>();
    for (const entry of entries.value) known.add(entry.id);
    for (const entry of pending.value) known.add(entry.id);
    const lowest = floor();
    const fresh = list.filter((entry) => !known.has(entry.id) && entry.id > lowest);
    if (fresh.length === 0) return;
    rememberAccounts(fresh);
    if (atTop.value) {
      setEntries(mergeDesc(entries.value, fresh));
      announce(fresh.length);
      return;
    }
    const merged = mergeDesc(pending.value, fresh);
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

  /** Reprend la liste à zéro avec le filtre appliqué. */
  async function load(): Promise<boolean> {
    const id = serverId.value;
    if (id === null) return false;
    generation += 1;
    const mine = generation;
    status.value = "loading";
    failure.value = null;
    liveBuffer = [];
    try {
      const page = await bridge().readAudit(id, plain(applied.value), null);
      if (mine !== generation) return false;
      const buffered = liveBuffer ?? [];
      liveBuffer = null;
      nextBefore.value = page.nextBefore;
      pending.value = [];
      pendingOverflow.value = false;
      atTop.value = true;
      setEntries(mergeDesc(page.events, isUnfiltered(applied.value) ? buffered : []));
      status.value = "ready";
      topSignal.value += 1;
      // Filtre actif : ce qui est arrivé pendant la lecture se relit (l'agent filtre).
      if (!isUnfiltered(applied.value) && buffered.length > 0) scheduleCatchUp();
      return true;
    } catch (error) {
      if (mine !== generation) return false;
      liveBuffer = null;
      status.value = "error";
      failure.value = failureOf(error);
      if (failure.value === null) logUiError(error, "audit");
      return false;
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
      failure.value = null;
    } catch (error) {
      if (mine !== generation) return;
      failure.value = failureOf(error);
      if (failure.value === null) logUiError(error, "audit");
    } finally {
      loadingMore.value = false;
    }
  }

  /**
   * Relit la tête du journal filtré jusqu'à retrouver ce qui est déjà là, et ajoute ce qui manque :
   * rattrapage après une coupure, et seule façon d'appliquer un filtre actif au direct. Trop
   * de pages à relire : on repart de la tête (la liste redevient une suite contiguë).
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
        if (entries.value.length === 0 && pending.value.length === 0) {
          await load();
          break;
        }
        const known = Math.max(top(), pending.value[0]?.id ?? 0);
        const found: AuditEntry[] = [];
        let before: number | null = null;
        let reached = false;
        let pages = 0;
        while (!reached && pages < AUDIT_CATCH_UP_MAX_PAGES) {
          const page = await bridge().readAudit(id, plain(applied.value), before);
          if (mine !== generation) return;
          pages += 1;
          found.push(...page.events);
          const lastId = page.events[page.events.length - 1]?.id ?? 0;
          reached = page.nextBefore === null || lastId <= known;
          before = page.nextBefore;
        }
        if (mine !== generation) return;
        if (!reached) {
          await load();
          break;
        }
        ingest(found);
        status.value = "ready";
        failure.value = null;
        if (resumed) {
          resumed = false;
          useToastsStore().push({ kind: "success", message: t("audit.linkBack") });
        }
      } while (catchAgain);
    } catch (error) {
      if (mine === generation) {
        failure.value = failureOf(error);
        if (failure.value === null) logUiError(error, "audit");
      }
    } finally {
      catching = false;
    }
  }

  function scheduleCatchUp() {
    clearTimeout(liveTimer);
    liveTimer = setTimeout(() => {
      void catchUp();
    }, AUDIT_LIVE_DEBOUNCE_MS);
  }

  /** Une entrée reçue en direct. */
  function onLive(entry: AuditEntry) {
    if (serverId.value === null) return;
    if (!isUnfiltered(applied.value)) {
      // Un filtre actif s'applique au direct : on laisse l'agent dire si cette entrée y répond.
      scheduleCatchUp();
      return;
    }
    if (liveBuffer) {
      if (liveBuffer.length < LIVE_BUFFER_MAX) liveBuffer.push(entry);
      return;
    }
    ingest([entry]);
  }

  async function open(id: string): Promise<void> {
    await close();
    serverId.value = id;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    const mine = id;
    // Écoute posée d'abord, lecture ensuite : aucune entrée ne tombe entre les deux.
    try {
      const off = await bridge().onAudit(id, onLive);
      if (serverId.value !== mine) off();
      else unsubscribe = off;
    } catch (error) {
      logUiError(error, "audit");
    }
    if (serverId.value === mine) await load();
  }

  async function close(): Promise<void> {
    generation += 1;
    unsubscribe?.();
    unsubscribe = null;
    clearTimeout(liveTimer);
    clearTimeout(announceTimer);
    announceTimer = undefined;
    serverId.value = null;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    entries.value = [];
    pending.value = [];
    pendingOverflow.value = false;
    nextBefore.value = null;
    status.value = "idle";
    failure.value = null;
    loadingMore.value = false;
    windowFull.value = false;
    knownAccounts.value = [];
    atTop.value = true;
    announcement.value = "";
    liveBuffer = null;
    catching = false;
    catchAgain = false;
    resumed = false;
    reloading.value = false;
    exporting.value = false;
  }

  /**
   * « Appliquer les filtres ». `invalid` : la période est invalide, rien n'est lancé. `failed` : la
   * lecture a échoué, les filtres précédents et la liste affichée restent (BR-AUDIT-014).
   */
  async function apply(
    draft: FilterDraft,
    now: number = Date.now(),
  ): Promise<"applied" | "invalid" | "failed"> {
    const filter = resolveFilter(cloneDraft(draft), now);
    if (!filter) return "invalid";
    const previous = applied.value;
    applied.value = filter;
    const ok = await load();
    if (ok) return "applied";
    applied.value = previous;
    return "failed";
  }

  /** « Effacer les filtres » : tout le journal ; `false` si la lecture échoue (l'état précédent reste). */
  async function clearFilters(): Promise<boolean> {
    const previous = applied.value;
    applied.value = { ...EMPTY_AUDIT_FILTER };
    const ok = await load();
    if (!ok) applied.value = previous;
    return ok;
  }

  /** Relit le journal avec les filtres appliqués (« Réessayer » après un échec de chargement). */
  async function reload(): Promise<void> {
    await load();
  }

  /** Le lien revient : on rattrape ce qui a été manqué (BR-AUDIT-011). */
  function resume(): void {
    if (serverId.value === null) return;
    resumed = true;
    if (status.value === "error" || entries.value.length === 0) {
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
    setEntries(mergeDesc(entries.value, held));
    topSignal.value += 1;
    if (overflow) await catchUp();
  }

  function setAtTop(value: boolean): void {
    if (atTop.value === value) return;
    atTop.value = value;
    // Revenu en haut à la main : ce qui attendait rejoint la liste, le bouton disparaît.
    if (value && (pending.value.length > 0 || pendingOverflow.value)) void showPending();
  }

  /** « Rechargement manuel » (BR-AUDIT-020) : une tentative de reconnexion, puis la relecture. */
  async function reloadManually(): Promise<void> {
    const id = serverId.value;
    if (id === null || reloading.value) return;
    reloading.value = true;
    const link = useLinkStore();
    try {
      await link.retryNow(id);
      const deadline = Date.now() + AUDIT_RELOAD_WAIT_MS;
      while (link.stateOf(id) !== "connected" && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 100));
        if (serverId.value !== id) return;
      }
      if (link.stateOf(id) !== "connected") {
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
    pending,
    pendingOverflow,
    newCount,
    atTop,
    windowFull,
    hasMoreBelow,
    knownAccounts,
    exporting,
    reloading,
    topSignal,
    announcement,
    open,
    close,
    apply,
    clearFilters,
    reload,
    loadMore,
    resume,
    showPending,
    setAtTop,
    reloadManually,
    exportCsv,
  };
});
