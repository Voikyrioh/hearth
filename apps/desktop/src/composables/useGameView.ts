import { computed, onBeforeUnmount, ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { failureOf, type GameView, getLinkBridge } from "@/link";

/** Ce que la lecture de la vue d'un serveur de jeu a donné. */
export type GameViewState =
  | { kind: "idle" }
  | { kind: "loading" }
  /** Ce service n'est pas un serveur de jeu créé depuis une fiche (BR-JEU-001) : aucune des quatre cartes. */
  | { kind: "absent" }
  | { kind: "found"; view: GameView }
  /** La lecture a échoué : on ne dit pas « pas un jeu », on n'invente rien. */
  | { kind: "failed" };

/**
 * La vue d'UN serveur de jeu (HRT-104, `BR-JEU-017` à `BR-JEU-021`) : lue quand le détail s'ouvre sur un service créé
 * par Hearth, relue quand ce qui peut la changer change (`refreshOn` : l'état du service, une opération qui se termine).
 * Le service suivi est repéré par sa CLÉ ; une réponse tardive d'un autre service est jetée. Une consultation ouverte à
 * tout compte : aucune ne demande le droit. Rien n'est gardé au-delà de la vie du détail.
 *
 * @param serverId le serveur du service.
 * @param key la clé du service, ou `null` : rien à lire (service adopté, en déploiement, lien coupé, agent sans la capacité).
 * @param refreshOn une valeur qui change quand la vue peut avoir changé.
 */
export function useGameView(
  serverId: () => string,
  key: () => string | null,
  refreshOn: () => string,
) {
  const state = ref<GameViewState>({ kind: "idle" });
  let ticket = 0;

  async function load(): Promise<void> {
    const target = key();
    const mine = ++ticket;
    if (target === null) {
      state.value = { kind: "idle" };
      return;
    }
    // Une relecture garde la vue montrée jusqu'à la réponse : les cartes ne clignotent pas.
    if (state.value.kind !== "found") state.value = { kind: "loading" };
    try {
      const read = await getLinkBridge().readGameView(serverId(), target);
      if (mine !== ticket) return;
      state.value = read.kind === "found" ? { kind: "found", view: read.view } : { kind: "absent" };
    } catch (error) {
      if (mine !== ticket) return;
      state.value = { kind: "failed" };
      if (!failureOf(error)) logUiError(error, "gameView:read");
    }
  }

  watch(
    () => [serverId(), key()] as const,
    () => {
      // Un autre service : la vue de l'ancien ne se montre jamais sous le nouveau.
      state.value = { kind: "idle" };
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

  return {
    state,
    view: computed(() => (state.value.kind === "found" ? state.value.view : null)),
    reload: load,
  };
}
