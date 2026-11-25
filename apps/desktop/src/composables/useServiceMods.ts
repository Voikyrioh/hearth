import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import {
  failureOf,
  getLinkBridge,
  MAX_BATCH_ADDS,
  type ModIdentity,
  type ModItem,
  type ModsBatch,
  type ModsReadRefusal,
  type ModsView,
  type ModVerdict,
  type ServiceInfo,
} from "@/link";
import { enableConflict, type Gesture } from "@/services/mods";

/** Ce que la lecture de la liste des mods a donné (BR-MOD-020). */
export type ModsListState =
  | { kind: "loading" }
  | { kind: "ready"; view: ModsView }
  | { kind: "refused"; refusal: ModsReadRefusal }
  | { kind: "failed" };

/** Un ajout contrôlé, prêt pour le lot : le dépôt, son empreinte, le nom que Hearth lui donnera et le fichier remplacé. */
export interface PendingAdd {
  deposit: string;
  sha256: string;
  identity: ModIdentity | null;
  verdict: ModVerdict | null;
  /** Le nom sous lequel Hearth le posera (choisi par Hearth, jamais le nom envoyé). */
  targetFile: string;
  /** Le fichier actif qu'il remplace (BR-MOD-023). */
  replace: string | null;
  replaceVersion: string | null;
}

/** Le lot d'un service : ses gestes, les raisons des gestes écartés et ses ajouts. */
interface KeptBatch {
  pending: Map<string, Gesture>;
  refusedNote: Map<string, string>;
  adds: PendingAdd[];
}

/**
 * Les lots non appliqués, gardés au niveau du service (HRT-262, constat B2) : changer d'onglet (« Ouvrir la console »)
 * ne perd jamais le lot ; seuls « Annuler tout », « Appliquer » ou une sortie confirmée du détail le vident. Un lot par
 * serveur et service (clé stable), jamais écrit ailleurs qu'en mémoire.
 */
const kept = reactive(new Map<string, KeptBatch>());

function keptKey(serverId: string, serviceKey: string): string {
  return `${serverId}|${serviceKey}`;
}

function keptOf(serverId: string, serviceKey: string): KeptBatch {
  const key = keptKey(serverId, serviceKey);
  let found = kept.get(key);
  if (!found) {
    kept.set(key, { pending: new Map(), refusedNote: new Map(), adds: [] });
    found = kept.get(key) as KeptBatch;
  }
  return found;
}

/** Ce service a un lot non appliqué (le détail demande confirmation avant de le quitter). */
export function hasKeptBatch(serverId: string, serviceKey: string): boolean {
  const found = kept.get(keptKey(serverId, serviceKey));
  return found !== undefined && found.pending.size + found.adds.length > 0;
}

/** Vide le lot d'un service (sortie confirmée). */
export function dropKeptBatch(serverId: string, serviceKey: string): void {
  kept.delete(keptKey(serverId, serviceKey));
}

/** Vide tous les lots (les tests, la déconnexion). */
export function dropAllKeptBatches(): void {
  kept.clear();
}

/** Quand des jars ne sont pas encore lus, la liste est relue après ce délai (BR-MOD-020 : « Lecture des mods en cours »). */
export const REREAD_MS = 3_000;

/**
 * Le parcours de l'onglet « Mods » d'un serveur Minecraft (HRT-261, HRT-262, `BR-MOD-020` à `033`) :
 *
 * - la liste est lue à l'ouverture, relue au retour du lien, et RELUE SEULE tant que des jars ne sont pas lus (`unread`) ;
 * - le LOT (désactiver, réactiver, retirer, remettre, ajouter) vit ICI, dans l'écran : rien n'est écrit sur le serveur
 *   avant « Appliquer » (BR-MOD-022, 025), et un lot non appliqué ne se partage pas. Un fichier n'est que dans un seul
 *   geste du lot ; refaire le geste sur la ligne ou « Annuler » le sort du lot ;
 * - après une relecture, un geste qui vise un fichier qui n'existe plus ou qui n'est plus dans le même état est écarté.
 *
 * Le service est suivi par sa CLÉ ; les noms de fichiers sont du texte, jamais du HTML.
 */
export function useServiceMods(
  serverId: () => string,
  service: () => ServiceInfo,
  connected: () => boolean,
) {
  const list = ref<ModsListState>({ kind: "loading" });
  const refreshing = ref(false);
  let reading = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const view = computed(() => (list.value.kind === "ready" ? list.value.view : null));

  /** Les ajouts contrôlés du lot : 8 au plus (BR-MOD-022). */
  const lot = keptOf(serverId(), service().key);
  const adds = computed({
    get: () => lot.adds,
    set: (value: PendingAdd[]) => {
      lot.adds = value;
    },
  });

  function stopTimer() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }

  /** Les gestes préparés sur un fichier existant : fichier -> geste. */
  const pending = computed({
    get: () => lot.pending,
    set: (value: Map<string, Gesture>) => {
      lot.pending = value;
    },
  });
  /** Pourquoi un geste n'est pas entré dans le lot (dit sous la ligne, jamais en silence). */
  const refusedNote = computed({
    get: () => lot.refusedNote,
    set: (value: Map<string, string>) => {
      lot.refusedNote = value;
    },
  });

  function prune(next: ModsView) {
    const present = new Map(next.mods.map((mod) => [mod.file, mod] as const));
    for (const [file, gesture] of [...pending.value]) {
      const item = present.get(file);
      const valid =
        item !== undefined &&
        ((gesture === "disable" && item.place === "active" && item.enabled) ||
          (gesture === "enable" && item.place === "active" && !item.enabled) ||
          (gesture === "retire" && item.place === "active") ||
          (gesture === "restore" && item.place === "retired"));
      if (!valid) pending.value.delete(file);
    }
    pending.value = new Map(pending.value);
  }

  async function load(): Promise<void> {
    const mine = ++reading;
    stopTimer();
    const had = list.value.kind === "ready";
    if (had) refreshing.value = true;
    else list.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readMods(serverId(), service().key);
      if (mine !== reading) return;
      if (outcome.kind === "ready") {
        list.value = { kind: "ready", view: outcome.view };
        prune(outcome.view);
        if (outcome.view.unread > 0) timer = setTimeout(() => void load(), REREAD_MS);
      } else {
        list.value = { kind: "refused", refusal: outcome.refusal };
      }
    } catch (error) {
      if (mine !== reading) return;
      if (!failureOf(error)) logUiError(error, "serviceMods:list");
      // Une relecture qui échoue garde la liste déjà lue ; une première lecture qui échoue ne devine rien.
      if (!had) list.value = { kind: "failed" };
    } finally {
      if (mine === reading) refreshing.value = false;
    }
  }

  void load();
  watch(connected, (now, before) => {
    if (now && !before) void load();
  });
  onBeforeUnmount(() => {
    reading += 1;
    stopTimer();
  });

  // ── Le lot : les gestes sur les fichiers de la liste ─────────────────────────────────────────

  function set(file: string, gesture: Gesture | null) {
    const next = new Map(pending.value);
    if (gesture === null) next.delete(file);
    else next.set(file, gesture);
    pending.value = next;
    const notes = new Map(refusedNote.value);
    notes.delete(file);
    refusedNote.value = notes;
  }

  /** L'interrupteur « Activé » : désactive un mod actif, réactive un mod désactivé ; refaire le geste le sort du lot. */
  function toggle(item: ModItem) {
    if (item.file === null || item.place !== "active") return;
    const current = pending.value.get(item.file);
    if (current !== undefined) {
      if (current === "disable" || current === "enable") return set(item.file, null);
    }
    if (item.enabled) return set(item.file, "disable");
    const current_view = view.value;
    const conflict = current_view ? enableConflict(item, current_view, pending.value) : null;
    if (conflict) {
      refusedNote.value = new Map(refusedNote.value).set(item.file, conflict);
      return;
    }
    set(item.file, "enable");
  }

  /** « Retirer » : met le fichier de côté dans « Mods retirés » (jamais supprimé, BR-MOD-024). */
  function retire(item: ModItem) {
    if (item.file === null || item.place !== "active") return;
    set(item.file, "retire");
  }

  /** « Remettre » un mod retiré. */
  function restore(item: ModItem) {
    if (item.file === null || item.place !== "retired") return;
    set(item.file, "restore");
  }

  /** « Annuler » sur une ligne : le geste sort du lot. */
  function undo(item: ModItem) {
    if (item.file !== null) set(item.file, null);
  }

  /** Met des ajouts contrôlés dans le lot (un dépôt n'y entre qu'une fois). */
  function addEntries(entries: readonly PendingAdd[]) {
    const known = new Set(adds.value.map((add) => add.deposit));
    const fresh = entries.filter((entry) => !known.has(entry.deposit));
    adds.value = [...adds.value, ...fresh].slice(0, MAX_BATCH_ADDS);
  }

  function removeAdd(deposit: string) {
    adds.value = adds.value.filter((add) => add.deposit !== deposit);
  }

  function clearAll() {
    pending.value = new Map();
    refusedNote.value = new Map();
    adds.value = [];
  }

  /** Les fichiers du lot par geste, tels que l'agent les reçoit (hors ajouts, qui viennent de HRT-262). */
  const batch = computed<ModsBatch>(() => {
    const out: ModsBatch = { add: [], retire: [], restore: [], disable: [], enable: [] };
    for (const [file, gesture] of pending.value) out[gesture].push(file);
    out.add = adds.value.map((add) => ({
      deposit: add.deposit,
      sha256: add.sha256,
      replace: add.replace,
    }));
    return out;
  });
  const changeCount = computed(() => pending.value.size + adds.value.length);
  /** Le fichier remplacé -> l'ajout qui le remplace (la ligne de l'ancien le dit). */
  const replacedBy = computed(
    () =>
      new Map(
        adds.value.flatMap((add) => (add.replace === null ? [] : [[add.replace, add] as const])),
      ),
  );

  return {
    list,
    view,
    refreshing,
    reload: load,
    pending,
    refusedNote,
    adds,
    replacedBy,
    addEntries,
    removeAdd,
    batch,
    changeCount,
    toggle,
    retire,
    restore,
    undo,
    clearAll,
  };
}
