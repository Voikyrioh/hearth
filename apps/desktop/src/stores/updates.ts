import { defineStore } from "pinia";
import { computed, ref } from "vue";
import type { UpdateStateDto } from "@/bindings";
import { logUiError } from "@/errors/report";
import { getUpdateBridge, type Unsubscribe } from "@/updates";

/** Ce que le bandeau montre : une annonce, un travail en cours, ou un échec. */
export type BannerKind = "available" | "downloading" | "installing" | "failed";

/**
 * Mise à jour du client, côté interface : l'état complet vient de la coquille (qui décide de la
 * fréquence, du report de 24 h et de la visibilité du bandeau) ; ici on le range, on écarte un état
 * plus ancien que le dernier vu (`seq`) et on relaie les clics. Rien n'est jamais stocké par la page :
 * ni `localStorage`, ni date (ADR-0017).
 *
 * Une vérification qui échoue n'est JAMAIS un message (BR-UPDATE-007, 008) : la commande rend l'état,
 * et si le pont lui-même est en panne, on se tait aussi (on le journalise).
 */
export const useUpdatesStore = defineStore("updates", () => {
  const state = ref<UpdateStateDto | null>(null);
  const checking = ref(false);
  let unsubscribe: Unsubscribe | null = null;
  let started: Promise<void> | null = null;

  function apply(next: UpdateStateDto | null | undefined) {
    // Un état plus ancien que le dernier vu (rattrapage en retard) est écarté ; le même est rejoué.
    if (!next || typeof next.seq !== "number") return;
    if (state.value && next.seq < state.value.seq) return;
    state.value = next;
  }

  /** S'abonne puis lit l'état (écoute d'abord : aucun état perdu entre les deux). */
  function start(): Promise<void> {
    started ??= (async () => {
      const bridge = getUpdateBridge();
      try {
        unsubscribe = await bridge.onState(apply);
        apply(await bridge.getState());
      } catch (error) {
        // Pas de pont (navigateur de revue) : pas de bandeau, pas de message.
        logUiError(error, "updates:start");
      }
    })();
    return started;
  }

  function stop() {
    unsubscribe?.();
    unsubscribe = null;
    started = null;
  }

  const banner = computed<BannerKind | null>(() => {
    const current = state.value;
    if (!current) return null;
    if (current.phase === "downloading") return "downloading";
    if (current.phase === "installing") return "installing";
    if (current.failure && current.available) return "failed";
    return current.bannerVisible && current.available ? "available" : null;
  });

  const available = computed(() => state.value?.available ?? null);
  const busy = computed(() => {
    const phase = state.value?.phase;
    return phase === "downloading" || phase === "installing";
  });

  async function run(action: () => Promise<UpdateStateDto>) {
    try {
      apply(await action());
    } catch (error) {
      logUiError(error, "updates:action");
    }
  }

  /** « Vérifier maintenant ». */
  async function checkNow() {
    if (checking.value || state.value?.phase === "checking") return;
    checking.value = true;
    try {
      await run(() => getUpdateBridge().check());
    } finally {
      checking.value = false;
    }
  }

  /** « Plus tard ». */
  function postpone() {
    return run(() => getUpdateBridge().postpone());
  }

  /** « Mettre à jour maintenant » (et « Réessayer ») : le clic de l'utilisateur. */
  function install() {
    if (busy.value) return Promise.resolve();
    return run(() => getUpdateBridge().install());
  }

  return { state, checking, banner, available, busy, start, stop, checkNow, postpone, install };
});
