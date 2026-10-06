import type {
  AgentUpdateEvent,
  AgentUpdateOutcome,
  AgentUpdateView,
  UpdateOutcome,
  UpdateProgress,
  UpdateReason,
  UpdateResult,
  UpdateStep,
} from "./agent-update";
import type { LinkState, Unsubscribe } from "./types";

/**
 * La mise à jour de l'agent SIMULÉE (navigateur de développement, tests) : un état par serveur
 * (version de l'agent, installation gérée, version publiée dans le flux, mise à jour en cours, dernier
 * résultat), les mêmes refus que le vrai agent (une seule à la fois, installation gérée) et un pilotage
 * à la main (`advance`, `complete`) : le test avance les étapes, comme l'agent le ferait. Aucune
 * règle de rôle ici : le pont les juge (comme l'agent, pas l'interface). Les règles réelles sont
 * prouvées contre un vrai agent côté Rust (`tests/agent_update_runtime.rs`).
 */
export interface SimAgentUpdateState {
  /** La version de l'agent installé. */
  current: string;
  /** Installation gérée par le système : pas de mise à jour à distance. */
  managed: boolean;
  /** La version que le flux de versions publie (`null` : aucune). */
  target: string | null;
  progress: UpdateProgress | null;
  last: UpdateResult | null;
}

interface Deps {
  now: () => number;
  /** Change l'état du lien du serveur (le redémarrage de l'agent coupe le lien, sans alarme). */
  publishLink: (serverId: string, state: LinkState) => void;
}

const DEFAULT_STATE: SimAgentUpdateState = {
  current: "0.1.0",
  managed: false,
  target: "0.2.0",
  progress: null,
  last: null,
};

/** Compare deux versions `X.Y.Z` (le pont simulé n'en connaît pas d'autres). */
function newer(a: string, b: string): boolean {
  const left = a.split(".").map(Number);
  const right = b.split(".").map(Number);
  for (let i = 0; i < 3; i += 1) {
    if ((left[i] ?? 0) !== (right[i] ?? 0)) return (left[i] ?? 0) > (right[i] ?? 0);
  }
  return false;
}

export class SimulatedAgentUpdates {
  private readonly states = new Map<string, SimAgentUpdateState>();
  private readonly listeners = new Set<(event: AgentUpdateEvent) => void>();
  private readonly deps: Deps;

  constructor(deps: Deps) {
    this.deps = deps;
  }

  /** L'état d'un serveur (créé au premier accès, avec les valeurs par défaut). */
  stateOf(serverId: string): SimAgentUpdateState {
    let state = this.states.get(serverId);
    if (!state) {
      state = { ...DEFAULT_STATE };
      this.states.set(serverId, state);
    }
    return state;
  }

  /** Amorçage d'un test : change l'état d'un serveur. */
  seed(serverId: string, change: Partial<SimAgentUpdateState>): void {
    Object.assign(this.stateOf(serverId), change);
  }

  view(serverId: string): AgentUpdateView {
    const state = this.stateOf(serverId);
    return {
      current: state.current,
      managed: state.managed,
      inProgress: state.progress !== null,
      progress: state.progress ? { ...state.progress } : null,
      last: state.last ? { ...state.last } : null,
      available:
        state.target && !state.managed && newer(state.target, state.current)
          ? { version: state.target }
          : null,
    };
  }

  /** La demande d'un administrateur : les refus de l'agent (installation gérée, déjà en cours). */
  start(serverId: string, version: string): AgentUpdateOutcome {
    const state = this.stateOf(serverId);
    if (state.target === null) return { kind: "refused", refusal: { kind: "no_target" } };
    if (state.target !== version) return { kind: "refused", refusal: { kind: "target_changed" } };
    if (!newer(state.target, state.current)) {
      return { kind: "refused", refusal: { kind: "not_newer" } };
    }
    if (state.managed) return { kind: "refused", refusal: { kind: "managed_install" } };
    if (state.progress) return { kind: "refused", refusal: { kind: "in_progress" } };
    this.emit(serverId, { version, step: "download", percent: null, outcome: null, reason: null });
    return { kind: "accepted", version };
  }

  /** L'agent avance d'une étape (`percent` pour le téléchargement). `restart` coupe le lien. */
  advance(
    serverId: string,
    step: Exclude<UpdateStep, "done">,
    percent: number | null = null,
  ): void {
    const state = this.stateOf(serverId);
    const version = state.progress?.version ?? state.target ?? state.current;
    this.emit(serverId, {
      version,
      step,
      percent: step === "download" ? percent : null,
      outcome: null,
      reason: null,
    });
    // Le redémarrage coupe le lien : « Reconnexion… », sans alarme (BR-UPDATE-014).
    if (step === "restart") this.deps.publishLink(serverId, "reconnecting");
  }

  /** L'agent conclut : son résultat est écrit, l'étape `done` annoncée, le lien revient. */
  complete(serverId: string, outcome: UpdateOutcome, reason: UpdateReason | null = null): void {
    const state = this.stateOf(serverId);
    const version = state.progress?.version ?? state.target ?? state.current;
    const previous = state.current;
    if (outcome === "succeeded") state.current = version;
    state.last = {
      version,
      previous,
      outcome,
      reason,
      at: new Date(this.deps.now()).toISOString(),
      recent: true,
    };
    state.progress = null;
    this.deps.publishLink(serverId, "connected");
    this.listenersEmit(serverId, { version, step: "done", percent: null, outcome, reason });
  }

  /** Le résultat est celui d'une version qui ne se sait pas (trace illisible conclue au démarrage). */
  completeUnknownVersion(serverId: string, reason: UpdateReason = "interrupted"): void {
    const state = this.stateOf(serverId);
    const previous = state.current;
    state.last = {
      version: null,
      previous,
      outcome: "failed",
      reason,
      at: new Date(this.deps.now()).toISOString(),
      recent: true,
    };
    state.progress = null;
    this.listenersEmit(serverId, {
      version: "",
      step: "done",
      percent: null,
      outcome: "failed",
      reason,
    });
  }

  subscribe(listener: (event: AgentUpdateEvent) => void): Unsubscribe {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  }

  private emit(serverId: string, progress: UpdateProgress): void {
    this.stateOf(serverId).progress = progress;
    this.listenersEmit(serverId, progress);
  }

  private listenersEmit(serverId: string, progress: UpdateProgress): void {
    for (const listener of [...this.listeners]) listener({ serverId, progress: { ...progress } });
  }
}
