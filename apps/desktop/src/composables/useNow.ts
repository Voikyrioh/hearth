import { onScopeDispose, readonly, ref } from "vue";

// Une seule horloge partagée : un seul minuteur pour toute l'interface, arrêté quand plus
// personne ne s'en sert (aucune accumulation en session longue, BR-RESIL-017).
const now = ref(Date.now());
let users = 0;
let timer: ReturnType<typeof setInterval> | undefined;

/** Heure courante (ms), rafraîchie chaque seconde tant qu'un composant l'utilise. */
export function useNow() {
  now.value = Date.now();
  users += 1;
  timer ??= setInterval(() => {
    now.value = Date.now();
  }, 1000);
  onScopeDispose(() => {
    users -= 1;
    if (users === 0 && timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  });
  return readonly(now);
}
