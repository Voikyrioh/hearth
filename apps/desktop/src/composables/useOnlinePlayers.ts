import { computed, onBeforeUnmount, ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { failureOf, getLinkBridge, type PlayersView } from "@/link";
import { PLAYERS_REFRESH_MS } from "@/services/players";

/** Ce que la lecture des joueurs connectés a donné. */
export type OnlinePlayersState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; view: PlayersView }
  /** Le service n'existe plus ou la lecture est refusée : aucune carte. */
  | { kind: "absent" }
  /** La lecture a échoué : rien n'est deviné, la prochaine lecture réessaie. */
  | { kind: "failed" };

/**
 * Les joueurs connectés d'UN serveur de jeu (HRT-291, `BR-CONSOLE-011`) : lus à l'ouverture de l'écran, relus toutes les
 * 30 secondes tant que l'écran existe, et à la demande (`refresh`). La lecture est ouverte à tout compte, sans mot de
 * passe ni journal ; l'agent partage une même lecture entre les écrans (10 secondes). Le service est suivi par sa CLÉ :
 * une réponse tardive d'un autre service est jetée ; une relecture qui échoue garde la dernière vue lue (datée par
 * `readAt`, jamais effacée en silence). Plus aucune lecture quand l'écran est fermé.
 *
 * @param key la clé du service, ou `null` : rien à lire (lien coupé, service adopté, agent sans la capacité).
 */
export function useOnlinePlayers(
  serverId: () => string,
  key: () => string | null,
  intervalMs: number = PLAYERS_REFRESH_MS,
) {
  const state = ref<OnlinePlayersState>({ kind: "idle" });
  const refreshing = ref(false);
  let ticket = 0;
  let timer: ReturnType<typeof setInterval> | undefined;

  async function load(): Promise<void> {
    const target = key();
    const mine = ++ticket;
    if (target === null) {
      state.value = { kind: "idle" };
      return;
    }
    if (state.value.kind !== "ready") state.value = { kind: "loading" };
    refreshing.value = true;
    try {
      const outcome = await getLinkBridge().readPlayers(serverId(), target);
      if (mine !== ticket) return;
      state.value =
        outcome.kind === "ready" ? { kind: "ready", view: outcome.view } : { kind: "absent" };
    } catch (error) {
      if (mine !== ticket) return;
      if (!failureOf(error)) logUiError(error, "players:read");
      if (state.value.kind !== "ready") state.value = { kind: "failed" };
    } finally {
      if (mine === ticket) refreshing.value = false;
    }
  }

  function disarm() {
    if (timer !== undefined) clearInterval(timer);
    timer = undefined;
  }

  watch(
    () => [serverId(), key()] as const,
    () => {
      // Un autre service : la vue de l'ancien ne se montre jamais sous le nouveau.
      disarm();
      state.value = { kind: "idle" };
      void load();
      if (key() !== null) timer = setInterval(() => void load(), intervalMs);
    },
    { immediate: true },
  );
  onBeforeUnmount(() => {
    ticket += 1;
    disarm();
  });

  return {
    state,
    view: computed(() => (state.value.kind === "ready" ? state.value.view : null)),
    refreshing,
    refresh: load,
  };
}
