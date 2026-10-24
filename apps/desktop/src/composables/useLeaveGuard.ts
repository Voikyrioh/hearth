import { onBeforeUnmount, onMounted, ref } from "vue";
import {
  onBeforeRouteLeave,
  type RouteLocationNormalized,
  type RouteLocationRaw,
  useRouter,
} from "vue-router";
import { useUnloadGuard } from "./useUnloadGuard";

/**
 * Toute la mécanique de sortie d'une page à saisie (formulaire de déploiement, installation d'une fiche) : une saisie en
 * cours ne se perd jamais sans la question « Abandonner votre saisie ? », quelle que soit la façon de partir. Fermer,
 * Échap hors d'un champ, précédent et menu passent par le routeur ; le rechargement par `beforeunload`
 * (`useUnloadGuard`, revue UX du 24/10, constat 7). Un déploiement lancé laisse partir sans question.
 *
 * - `dirty` : une saisie existe ; `launching` : l'opération est partie (la saisie est celle de la demande) ;
 * - `home` : où mène « Fermer » ; `wipe` : efface la saisie de la mémoire à l'abandon.
 * - `leave(to)` : part sans question (lancement, moteur devenu inutilisable) ; l'autorisation retombe si la navigation
 *   n'aboutit pas, pour que la garde ne reste pas désarmée.
 */
export function useLeaveGuard(options: {
  dirty: () => boolean;
  launching: () => boolean;
  home: () => RouteLocationRaw;
  wipe: () => void;
}) {
  const router = useRouter();
  const abandonOpen = ref(false);
  let allowed = false;
  let pending: RouteLocationNormalized | null = null;

  onBeforeRouteLeave((to) => {
    if (allowed || options.launching() || !options.dirty()) return true;
    pending = to;
    abandonOpen.value = true;
    return false;
  });

  useUnloadGuard(() => !allowed && !options.launching() && options.dirty());

  async function leave(to: RouteLocationRaw, replace = false): Promise<void> {
    allowed = true;
    const failure = await (replace ? router.replace(to) : router.push(to));
    if (failure) allowed = false;
  }

  function requestClose(): void {
    if (options.dirty()) {
      pending = null;
      abandonOpen.value = true;
      return;
    }
    void leave(options.home());
  }

  function abandon(): void {
    options.wipe();
    abandonOpen.value = false;
    void leave(pending ?? options.home());
  }

  function keepTyping(): void {
    abandonOpen.value = false;
    pending = null;
  }

  /** Échap hors d'un champ ferme la page ; dans un champ ou une fenêtre ouverte, rien. */
  function onKey(event: KeyboardEvent): void {
    if (event.key !== "Escape" || document.querySelector("dialog[open]")) return;
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName))
    ) {
      return;
    }
    requestClose();
  }
  onMounted(() => document.addEventListener("keydown", onKey));
  onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

  return { abandonOpen, leave, requestClose, abandon, keepTyping };
}
