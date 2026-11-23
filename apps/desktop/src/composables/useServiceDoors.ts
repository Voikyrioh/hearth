import { onBeforeUnmount, ref, watch } from "vue";
import { readGameOnce } from "@/composables/useGameView";
import { logUiError } from "@/errors/report";
import { type ConsoleView, failureOf, getLinkBridge } from "@/link";

/**
 * Les portes des onglets « Mods » et « Console » d'un serveur de jeu (HRT-261, HRT-269, BR-MOD-001, BR-CONSOLE-002) :
 *
 * - « Mods » existe quand la FICHE du service offre la voie `drop_jars` (même pour un moteur sans mods : l'onglet le dit,
 *   BR-MOD-030) ; la fiche vient de la vue du jeu (`GET /services/{key}/game`) puis du catalogue ;
 * - « Console » existe quand l'agent parle à la console de ce service (`GET /services/{key}/console`, sans rien envoyer
 *   au jeu : `available`).
 *
 * Aucune des deux lectures n'invente : une lecture qui échoue ne montre aucun onglet de plus, la suivante réessaie.
 *
 * @param key la clé du service, ou `null` : rien à lire (service adopté, en déploiement, lien coupé, disparu).
 * @param consoleServed l'agent annonce `game-console`.
 * @param refreshOn une valeur qui change quand les portes ont pu changer (état du service, opération qui se termine).
 */
export function useServiceDoors(
  serverId: () => string,
  key: () => string | null,
  consoleServed: () => boolean,
  refreshOn: () => string,
) {
  const hasMods = ref(false);
  const console = ref<ConsoleView | null>(null);
  let ticket = 0;

  async function loadMods(target: string, mine: number): Promise<void> {
    const game = await readGameOnce(serverId(), target);
    if (mine !== ticket) return;
    if (game.kind !== "found") {
      hasMods.value = false;
      return;
    }
    try {
      const read = await getLinkBridge().readFiche(serverId(), game.view.fiche);
      if (mine !== ticket) return;
      hasMods.value = read.kind === "found" && read.fiche.mods?.ways.includes("drop_jars") === true;
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "serviceDoors:fiche");
    }
  }

  async function loadConsole(target: string, mine: number): Promise<void> {
    try {
      const read = await getLinkBridge().describeConsole(serverId(), target);
      if (mine !== ticket) return;
      console.value = read.kind === "ready" && read.view.available ? read.view : null;
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "serviceDoors:console");
    }
  }

  async function load(): Promise<void> {
    const target = key();
    const mine = ++ticket;
    if (target === null) {
      hasMods.value = false;
      console.value = null;
      return;
    }
    await Promise.all([
      loadMods(target, mine),
      consoleServed() ? loadConsole(target, mine) : Promise.resolve(),
    ]);
  }

  watch(
    () => [serverId(), key(), consoleServed()] as const,
    () => {
      // Un autre service : les portes de l'ancien ne se montrent jamais sous le nouveau.
      hasMods.value = false;
      console.value = null;
      void load();
    },
    { immediate: true },
  );
  watch(refreshOn, () => {
    if (key() !== null) void load();
  });
  onBeforeUnmount(() => {
    ticket += 1;
  });

  return { hasMods, console, reload: load };
}
