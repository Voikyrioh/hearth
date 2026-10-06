import type { LinkBridge } from "./bridge";
import { NullLinkBridge } from "./null";
import { SAMPLE_AGENT, SimulatedLinkBridge } from "./simulated";
import { TauriLinkBridge } from "./tauri";

export * from "./audit";
export type { LinkBridge } from "./bridge";
export * from "./machine";
export * from "./messages";
export {
  groupFingerprint,
  OTHER_FINGERPRINT,
  SAMPLE_AGENT,
  SAMPLE_FINGERPRINT,
  SAMPLE_SERVERS,
  type SimAgent,
  SimulatedLinkBridge,
  type SimulatedOptions,
} from "./simulated";
export { SimulatedAudit } from "./simulated-audit";
export { bareMachine, type Pinnable, SimulatedMachine, sampleMachine } from "./simulated-machine";
export { TauriLinkBridge } from "./tauri";
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
 * - Fenêtre Tauri : le pont réel (commandes et événements `link://*` de la coquille, qui les
 *   tient de `hearth-link`).
 * - Navigateur en mode développement (`vite`) : pont simulé, pilotable (panneau de
 *   développement, `window.__hearthSim`), avec un agent d'exemple (`SAMPLE_AGENT`). Ce code est
 *   retiré du binaire livré.
 * - Sinon (build livré servi dans un navigateur) : pont vide, où toute commande échoue.
 */
export function createLinkBridge(): LinkBridge {
  if (insideTauri()) return new TauriLinkBridge();
  if (import.meta.env.DEV) {
    // `?servers=none` : navigateur de revue sans serveur enregistré (écran d'accueil).
    const none = new URLSearchParams(window.location.search).get("servers") === "none";
    const simulated = new SimulatedLinkBridge({
      ...(none ? { servers: [] } : {}),
      agents: [SAMPLE_AGENT],
      liveMetrics: true,
    });
    (window as unknown as { __hearthSim?: SimulatedLinkBridge }).__hearthSim = simulated;
    return simulated;
  }
  return new NullLinkBridge();
}
