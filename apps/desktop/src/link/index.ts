import type { LinkBridge } from "./bridge";
import { NullLinkBridge } from "./null";
import { SimulatedLinkBridge } from "./simulated";

export type { LinkBridge } from "./bridge";
export { SAMPLE_SERVERS, SimulatedLinkBridge, type SimulatedOptions } from "./simulated";
export * from "./types";

let current: LinkBridge | null = null;

/** Pont utilisé par les stores. Les tests en installent un avec `setLinkBridge`. */
export function getLinkBridge(): LinkBridge {
  current ??= createLinkBridge();
  return current;
}

export function setLinkBridge(bridge: LinkBridge | null): void {
  current = bridge;
}

/** Vrai quand l'interface tourne dans la fenêtre Tauri (et non dans un navigateur). */
function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Point de branchement : choisit le pont de l'application.
 * - Navigateur en mode développement (`vite`) : pont simulé, pilotable (panneau de
 *   développement, `window.__hearthSim`). Ce code est retiré du binaire livré.
 * - Sinon : pont vide. Le pont réel (commandes et événements `link://*` de
 *   `hearth-link`) sera branché ici par le ticket qui fusionne la bibliothèque de liaison.
 */
export function createLinkBridge(): LinkBridge {
  if (import.meta.env.DEV && !insideTauri()) {
    // `?servers=none` : navigateur de revue sans serveur enregistré (écran d'accueil).
    const none = new URLSearchParams(window.location.search).get("servers") === "none";
    const simulated = new SimulatedLinkBridge(none ? { servers: [] } : {});
    (window as unknown as { __hearthSim?: SimulatedLinkBridge }).__hearthSim = simulated;
    return simulated;
  }
  return new NullLinkBridge();
}
