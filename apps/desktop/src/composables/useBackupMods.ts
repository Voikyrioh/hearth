import { onBeforeUnmount, ref } from "vue";
import { readGameOnce } from "@/composables/useGameView";
import { logUiError } from "@/errors/report";
import { type BackupMods, type BackupModsOutcome, failureOf, getLinkBridge } from "@/link";
import { addressesOf } from "@/services/game";
import { useServersStore } from "@/stores/servers";

/** Quand la comparaison est partielle (des jars actuels pas encore lus), elle est relue après ce délai (BR-MOD-036). */
export const COMPARE_REREAD_MS = 3_000;

/** La liste des mods d'UNE sauvegarde, de sa lecture à sa comparaison. */
export interface BackupModsEntry {
  kind: "loading" | "ready" | "refused" | "failed";
  view: BackupMods | null;
  /** « Comparer avec les mods actuels » a été demandé : la comparaison est lue (et relue tant qu'elle est partielle). */
  comparing: boolean;
}

/**
 * Les mods de chaque sauvegarde d'un serveur à mods (HRT-294, `BR-MOD-034` à `036`) : la liste est lue quand la ligne se
 * déplie (une consultation ouverte à qui voit les sauvegardes, rien n'est écrit) ; « Comparer avec les mods actuels »
 * demande la comparaison que l'AGENT calcule (jamais le client) et la relit seule tant qu'elle est partielle, jusqu'à ce
 * que la ligne se replie. Aucune sauvegarde n'est modifiée. L'adresse du serveur, pour « Copier la liste », est lue une
 * fois, à la première ouverture.
 */
export function useBackupMods(serverId: () => string, serviceKey: () => string) {
  const states = ref(new Map<string, BackupModsEntry>());
  const address = ref<string | null>(null);
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  let addressRead = false;

  function patch(id: string, next: Partial<BackupModsEntry>) {
    const current = states.value.get(id) ?? { kind: "loading", view: null, comparing: false };
    states.value = new Map(states.value).set(id, { ...current, ...next });
  }

  function stop(id: string) {
    const timer = timers.get(id);
    if (timer !== undefined) clearTimeout(timer);
    timers.delete(id);
  }

  async function read(id: string, compare: boolean): Promise<void> {
    let outcome: BackupModsOutcome;
    try {
      outcome = await getLinkBridge().readBackupMods(serverId(), serviceKey(), id, compare);
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "backupMods:read");
      if (states.value.get(id)?.view === null || states.value.get(id) === undefined) {
        patch(id, { kind: "failed" });
      }
      return;
    }
    // Une ligne repliée entre-temps ne reçoit plus rien.
    if (!states.value.has(id)) return;
    if (outcome.kind !== "ready") {
      patch(id, { kind: "refused", comparing: false });
      return;
    }
    patch(id, { kind: "ready", view: outcome.view });
    const compared = outcome.view.compare;
    if (compare && compared !== null && !compared.complete) {
      stop(id);
      timers.set(
        id,
        setTimeout(() => void read(id, true), COMPARE_REREAD_MS),
      );
    }
  }

  async function readAddress() {
    if (addressRead) return;
    addressRead = true;
    const game = await readGameOnce(serverId(), serviceKey());
    if (game.kind !== "found") return;
    const found = addressesOf(
      useServersStore().byId(serverId())?.host ?? "",
      game.view.ports,
      game.view.publicAddress,
      game.view.lanAddress,
      game.view.detectedAddress,
    );
    address.value = found?.internet?.text ?? found?.lan ?? null;
  }

  /** La ligne d'une sauvegarde se déplie : sa liste est lue. */
  function open(id: string): void {
    states.value = new Map(states.value).set(id, { kind: "loading", view: null, comparing: false });
    void read(id, false);
    void readAddress();
  }

  /** La ligne se replie : plus aucune relecture. */
  function close(id: string): void {
    stop(id);
    const next = new Map(states.value);
    next.delete(id);
    states.value = next;
  }

  /** « Comparer avec les mods actuels » : la comparaison de l'agent, relue tant qu'elle est partielle. */
  function compare(id: string): void {
    stop(id);
    patch(id, { comparing: true });
    void read(id, true);
  }

  /** Cache la comparaison (la liste reste). */
  function hideCompare(id: string): void {
    stop(id);
    const current = states.value.get(id);
    if (!current) return;
    patch(id, {
      comparing: false,
      view: current.view ? { ...current.view, compare: null } : null,
    });
  }

  onBeforeUnmount(() => {
    for (const timer of timers.values()) clearTimeout(timer);
    timers.clear();
  });

  return { states, address, open, close, compare, hideCompare };
}
