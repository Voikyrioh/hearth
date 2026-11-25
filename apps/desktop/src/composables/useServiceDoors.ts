import { onBeforeUnmount, ref, watch } from "vue";
import { readGameOnce } from "@/composables/useGameView";
import { logUiError } from "@/errors/report";
import { type ConsoleView, failureOf, getLinkBridge, type ModsGame } from "@/link";
import { consoleSaveOf, type StopWay, stopWayOf } from "@/services/game";

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
  /** Ce que l'on sait du moteur (Minecraft Java, Fabric, version) : lu avec la porte « Mods », pour l'en-tête de tout onglet. */
  const game = ref<ModsGame | null>(null);
  /** Comment ce jeu enregistre son monde à l'arrêt, d'après sa fiche : la source unique des phrases sur l'enregistrement. */
  const stopWay = ref<StopWay | null>(null);
  /** Hearth enregistre le monde par la console avant l'arrêt, d'après la fiche (`null` : pas lue). */
  const consoleSave = ref<boolean | null>(null);
  const console = ref<ConsoleView | null>(null);
  let ticket = 0;

  async function loadMods(target: string, mine: number): Promise<void> {
    const game = await readGameOnce(serverId(), target);
    if (mine !== ticket) return;
    stopWay.value = game.kind === "found" ? stopWayOf(game.view) : null;
    consoleSave.value = game.kind === "found" ? consoleSaveOf(game.view) : null;
    if (game.kind !== "found") {
      hasMods.value = false;
      return;
    }
    try {
      const read = await getLinkBridge().readFiche(serverId(), game.view.fiche);
      if (mine !== ticket) return;
      hasMods.value = read.kind === "found" && read.fiche.mods?.ways.includes("drop_jars") === true;
      if (hasMods.value) await loadGame(target, mine);
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "serviceDoors:fiche");
    }
  }

  /** Le moteur du service, lu dès l'ouverture (jamais par l'onglet « Mods » seul) : une lecture qui échoue n'invente rien. */
  async function loadGame(target: string, mine: number): Promise<void> {
    try {
      const outcome = await getLinkBridge().readMods(serverId(), target);
      if (mine !== ticket) return;
      game.value = outcome.kind === "ready" ? outcome.view.game : null;
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "serviceDoors:game");
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
      game.value = null;
      stopWay.value = null;
      consoleSave.value = null;
      console.value = null;
      return;
    }
    await Promise.all([
      loadMods(target, mine),
      consoleServed() ? loadConsole(target, mine) : Promise.resolve(),
    ]);
  }

  watch(
    [serverId, key, consoleServed],
    () => {
      // Un autre service : les portes de l'ancien ne se montrent jamais sous le nouveau.
      hasMods.value = false;
      game.value = null;
      stopWay.value = null;
      consoleSave.value = null;
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

  return { hasMods, game, stopWay, consoleSave, console, reload: load };
}
