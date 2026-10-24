import { onBeforeUnmount, onMounted } from "vue";

/**
 * Prévient avant qu'un rechargement ou une fermeture de la page efface une saisie en cours (revue UX 2026-10-24,
 * constat 7) : les quatre autres façons de quitter (Fermer, Échap, précédent, menu) posent déjà « Abandonner votre
 * saisie ? » par le routeur, mais le rechargement n'y passe pas. `dirty` est relu à chaque tentative : la question ne
 * se pose que si une saisie existe encore. Navigateur : question native de `beforeunload`. Fenêtre Tauri (WebView2) :
 * Ctrl+R et F5 passent par la même question native ; la croix de la fenêtre ne décharge jamais la page, elle la cache
 * (BR-CLIENT-004), donc la saisie y survit.
 */
export function useUnloadGuard(dirty: () => boolean): void {
  function onBeforeUnload(event: BeforeUnloadEvent): void {
    if (!dirty()) return;
    // Les deux gestes que les navigateurs acceptent pour déclencher la question (le texte n'est jamais montré).
    event.preventDefault();
    event.returnValue = "";
  }
  onMounted(() => window.addEventListener("beforeunload", onBeforeUnload));
  onBeforeUnmount(() => window.removeEventListener("beforeunload", onBeforeUnload));
}
