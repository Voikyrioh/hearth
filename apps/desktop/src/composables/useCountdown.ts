import { computed, onScopeDispose, ref } from "vue";

/**
 * Compte à rebours en secondes entières (« Trop de tentatives. Attends {n} s… », BR-CONN-006).
 * `start(n)` le (re)lance ; `remaining` vaut 0 une fois fini ; le minuteur s'arrête seul, et avec
 * la portée qui l'a créé.
 */
export function useCountdown() {
  const remaining = ref(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  function stop() {
    clearInterval(timer);
    timer = undefined;
  }

  function start(seconds: number) {
    stop();
    remaining.value = Math.max(0, Math.ceil(seconds));
    if (remaining.value === 0) return;
    timer = setInterval(() => {
      remaining.value -= 1;
      if (remaining.value <= 0) {
        remaining.value = 0;
        stop();
      }
    }, 1000);
  }

  onScopeDispose(stop);
  return { remaining, active: computed(() => remaining.value > 0), start, stop };
}
