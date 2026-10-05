import { computed } from "vue";
import { coverageMs, type Point, resample } from "@/dashboard/series";
import type { MachineSample } from "@/link";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";

/**
 * Courbe d'une mesure sur la fenêtre choisie, redessinée quand un échantillon arrive (`tick`) ou
 * que la fenêtre change, et seulement alors. Le rééchantillonnage (1 s, 1 s, 10 s) est fait ici,
 * sur l'historique gardé par le store (BR-DASH-010).
 */
export function useMachineSeries(
  entry: ServerMachine,
  pick: (sample: MachineSample) => number | null,
) {
  const store = useDashboardStore();
  return computed<Point[]>(() => {
    void entry.tick;
    return resample(entry.ring.samples(), store.windowKey, pick);
  });
}

/** Durée que l'historique couvre vraiment (ms). */
export function useCoverage(entry: ServerMachine) {
  const store = useDashboardStore();
  return computed(() => {
    void entry.tick;
    return coverageMs(entry.ring.samples(), store.windowKey);
  });
}
