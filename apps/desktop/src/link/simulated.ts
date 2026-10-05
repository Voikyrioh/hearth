import type { LinkBridge } from "./bridge";
import type { LinkState, LinkStateEvent, OperationEvent, ServerInfo, Unsubscribe } from "./types";

/** Serveurs d'exemple du pont simulé (navigateur de développement, tests). */
export const SAMPLE_SERVERS: ServerInfo[] = [
  { id: "forge", name: "forge", address: "192.168.1.120", color: 1, role: "admin" },
  { id: "salon", name: "nas-salon", address: "192.168.1.30:7443", color: 3, role: "readonly" },
];

export interface SimulatedOptions {
  servers?: ServerInfo[];
  /** Horloge injectable (millisecondes). */
  now?: () => number;
  /** Délai simulé d'une tentative « Réessayer maintenant » avant le retour à connecté ; négatif = jamais. */
  retryDelayMs?: number;
}

/**
 * Pont simulé complet : états pilotables par code (`setState`) ou par le panneau de
 * développement. Tous les serveurs démarrent « Connecté ».
 */
export class SimulatedLinkBridge implements LinkBridge {
  private servers: ServerInfo[];
  private readonly events = new Map<string, LinkStateEvent>();
  private readonly seqs = new Map<string, number>();
  private readonly stateListeners = new Set<(event: LinkStateEvent) => void>();
  private readonly serverListeners = new Set<(servers: ServerInfo[]) => void>();
  private readonly operationListeners = new Set<(event: OperationEvent) => void>();
  private readonly now: () => number;
  private readonly retryDelayMs: number;
  /** Serveurs pour lesquels « Réessayer maintenant » a été demandé (observable dans les tests). */
  readonly retries: string[] = [];

  constructor(options: SimulatedOptions = {}) {
    this.now = options.now ?? Date.now;
    this.retryDelayMs = options.retryDelayMs ?? 1500;
    this.servers = (options.servers ?? SAMPLE_SERVERS).map((server) => ({ ...server }));
    for (const server of this.servers) this.events.set(server.id, this.connectedEvent(server.id));
  }

  async onServersChanged(listener: (servers: ServerInfo[]) => void): Promise<Unsubscribe> {
    this.serverListeners.add(listener);
    listener(this.servers.map((server) => ({ ...server })));
    return () => void this.serverListeners.delete(listener);
  }

  async onLinkState(listener: (event: LinkStateEvent) => void): Promise<Unsubscribe> {
    this.stateListeners.add(listener);
    for (const event of this.events.values()) listener({ ...event });
    return () => void this.stateListeners.delete(listener);
  }

  async retryNow(serverId: string): Promise<void> {
    this.retries.push(serverId);
    this.setState(serverId, "reconnecting");
    if (this.retryDelayMs >= 0) {
      setTimeout(() => this.setState(serverId, "connected"), this.retryDelayMs);
    }
  }

  async onOperation(listener: (event: OperationEvent) => void): Promise<Unsubscribe> {
    this.operationListeners.add(listener);
    return () => void this.operationListeners.delete(listener);
  }

  // --- Pilotage (code de test, panneau de développement) ---

  /** Fait passer un serveur dans `state` et émet l'événement correspondant. */
  setState(serverId: string, state: LinkState): void {
    const previous = this.events.get(serverId);
    const now = this.now();
    const event: LinkStateEvent = {
      serverId,
      seq: this.nextSeq(serverId),
      state,
      since: now,
      lastContactAt: state === "connected" ? now : (previous?.lastContactAt ?? null),
      nextRetryAt: state === "reconnecting" || state === "offline" ? now + 5000 : null,
    };
    this.events.set(serverId, event);
    for (const listener of [...this.stateListeners]) listener({ ...event });
  }

  /** Émet une issue d'opération. */
  emitOperation(event: OperationEvent): void {
    for (const listener of [...this.operationListeners]) listener({ ...event });
  }

  addServer(server: ServerInfo): void {
    this.servers = [...this.servers, { ...server }];
    const event = this.connectedEvent(server.id);
    this.events.set(server.id, event);
    this.emitServers();
    for (const listener of [...this.stateListeners]) listener({ ...event });
  }

  removeServer(serverId: string): void {
    this.servers = this.servers.filter((server) => server.id !== serverId);
    this.events.delete(serverId);
    this.emitServers();
  }

  private emitServers(): void {
    for (const listener of [...this.serverListeners]) {
      listener(this.servers.map((server) => ({ ...server })));
    }
  }

  private connectedEvent(serverId: string): LinkStateEvent {
    const now = this.now();
    return {
      serverId,
      seq: this.nextSeq(serverId),
      state: "connected",
      since: now,
      lastContactAt: now,
      nextRetryAt: null,
    };
  }

  private nextSeq(serverId: string): number {
    const next = (this.seqs.get(serverId) ?? 0) + 1;
    this.seqs.set(serverId, next);
    return next;
  }
}
