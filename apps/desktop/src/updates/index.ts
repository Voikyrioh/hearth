import type { UpdateBridge } from "./bridge";
import { NullUpdateBridge } from "./null";
import { SimulatedUpdateBridge } from "./simulated";
import { TauriUpdateBridge } from "./tauri";

export type { Unsubscribe, UpdateBridge } from "./bridge";
export {
  type InstallOutcome,
  SimulatedUpdateBridge,
  type SimulatedUpdateOptions,
} from "./simulated";
export { TauriUpdateBridge, UPDATE_STATE_EVENT } from "./tauri";

let current: UpdateBridge | null = null;

/** Pont utilisé par le store. Les tests en installent un avec `setUpdateBridge`. */
export function getUpdateBridge(): UpdateBridge {
  current ??= createUpdateBridge();
  return current;
}

export function setUpdateBridge(bridge: UpdateBridge | null): void {
  current = bridge;
}

/** Vrai quand l'interface tourne dans la fenêtre Tauri (et non dans un navigateur). */
function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Point de branchement : fenêtre Tauri = pont réel ; navigateur en développement (`vite`) = pont
 * simulé, pilotable par `window.__hearthUpdateSim` (code retiré du binaire livré) ; sinon pont vide.
 */
export function createUpdateBridge(): UpdateBridge {
  if (insideTauri()) return new TauriUpdateBridge();
  if (import.meta.env.DEV) {
    const simulated = new SimulatedUpdateBridge();
    (window as unknown as { __hearthUpdateSim?: SimulatedUpdateBridge }).__hearthUpdateSim =
      simulated;
    return simulated;
  }
  return new NullUpdateBridge();
}
