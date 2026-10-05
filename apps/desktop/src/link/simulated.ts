import type { LinkBridge } from "./bridge";
import {
  type ActionInput,
  type ActionResult,
  DEFAULT_PORT,
  type FingerprintChange,
  LinkCommandError,
  type LinkFailure,
  type LinkNotice,
  type LinkState,
  type LinkStateEvent,
  type NewServerInput,
  type OperationEvent,
  type ProbeResult,
  type Role,
  type ServerEdit,
  type ServerInfo,
  type Unsubscribe,
} from "./types";

/** Serveurs d'exemple du pont simulé (navigateur de développement, tests). */
export const SAMPLE_SERVERS: ServerInfo[] = [
  {
    id: "forge",
    name: "forge",
    address: "192.168.1.120",
    host: "192.168.1.120",
    port: DEFAULT_PORT,
    color: 1,
    role: "admin",
    username: "marie",
    remember: true,
  },
  {
    id: "salon",
    name: "nas-salon",
    address: "192.168.1.30:7443",
    host: "192.168.1.30",
    port: 7443,
    color: 3,
    role: "readonly",
    username: "paul",
    remember: false,
  },
];

/** Un agent du réseau simulé : ce que `probeServer` trouve à une adresse. */
export interface SimAgent {
  host: string;
  port?: number;
  /** Empreinte, forme complète (64 caractères hexadécimaux). */
  fingerprint: string;
  machineName?: string;
  /** Comptes : identifiant vers mot de passe et rôle. */
  users?: Record<string, { password: string; role: Role }>;
  /** Version incompatible : l'agent est trop ancien, ou c'est le client. */
  incompatible?: "agent" | "client";
}

/** Empreintes d'exemple (le réseau simulé de développement et des tests). */
export const SAMPLE_FINGERPRINT =
  "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";
export const OTHER_FINGERPRINT = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";

/** Agent d'exemple joignable en développement : `192.168.1.50`, compte `marie` / `Correct-Horse-9`. */
export const SAMPLE_AGENT: SimAgent = {
  host: "192.168.1.50",
  fingerprint: SAMPLE_FINGERPRINT,
  machineName: "atelier",
  users: { marie: { password: "Correct-Horse-9", role: "admin" } },
};

export interface SimulatedOptions {
  servers?: ServerInfo[];
  /** Agents du réseau simulé (par défaut : aucun). */
  agents?: SimAgent[];
  /** Horloge injectable (millisecondes). */
  now?: () => number;
  /** Délai simulé d'une tentative « Réessayer maintenant » avant le retour à connecté ; négatif = jamais. */
  retryDelayMs?: number;
  /** Délai simulé des commandes réseau (sonde, connexion) ; 0 = immédiat. */
  latencyMs?: number;
}

/** 8 groupes de 4 caractères hexadécimaux majuscules (les 16 premiers octets). */
export function groupFingerprint(hex: string): string {
  const head = hex.toUpperCase().slice(0, 32);
  return (head.match(/.{1,4}/g) ?? []).join(" ");
}

const MAX_FAILED_LOGINS = 5;
const THROTTLE_SECONDS = 30;

/**
 * Pont simulé complet : états pilotables par code (`setState`) ou par le panneau de
 * développement, et un petit réseau d'agents simulés (`agents`) pour les parcours d'ajout, de
 * connexion et d'empreinte changée. Tous les serveurs de départ sont « Connecté ».
 */
export class SimulatedLinkBridge implements LinkBridge {
  private servers: ServerInfo[];
  private readonly events = new Map<string, LinkStateEvent>();
  private readonly seqs = new Map<string, number>();
  private readonly stateListeners = new Set<(event: LinkStateEvent) => void>();
  private readonly serverListeners = new Set<(servers: ServerInfo[]) => void>();
  private readonly operationListeners = new Set<(event: OperationEvent) => void>();
  private readonly fingerprintListeners = new Set<(change: FingerprintChange) => void>();
  private readonly noticeListeners = new Set<(notice: LinkNotice) => void>();
  private readonly now: () => number;
  private readonly retryDelayMs: number;
  private readonly latencyMs: number;
  private readonly agents: SimAgent[];
  /** Alertes d'empreinte en attente de décision (un état, rejoué à l'abonnement). */
  private readonly alerts = new Map<string, FingerprintChange>();
  /** Annoncés avant tout abonné : rejoués une seule fois au premier abonnement. */
  private unreadNotices: LinkNotice[] = [];
  private noticeIds = 0;
  private unreadOperations: OperationEvent[] = [];
  /** Empreinte épinglée de chaque serveur du carnet. */
  private readonly pins = new Map<string, string>();
  private readonly failedLogins = new Map<string, number>();
  private nextServer = 1;
  /** Serveurs pour lesquels « Réessayer maintenant » a été demandé (observable dans les tests). */
  readonly retries: string[] = [];
  /** Journal des commandes reçues (observable dans les tests) : jamais de mot de passe. */
  readonly calls: string[] = [];
  /** Dernier mot de passe mémorisé « au coffre » par serveur (observable dans les tests). */
  readonly vault = new Map<string, string>();
  /**
   * Ce que fait la prochaine action : `ok` répond 200 ; `cut` coupe le lien avant la réponse
   * (« Reconnexion… », résultat inconnu, BR-RESIL-009).
   */
  actionMode: "ok" | "cut" = "ok";
  /** Dernière clé d'opération donnée à une action restée sans réponse (observable dans les tests). */
  lastUnknownOpId: string | null = null;
  /** Serveur affiché dans la fenêtre, tel que la coquille l'a reçu (observable dans les tests). */
  displayedServer: string | null = null;
  private nextOperation = 1;

  constructor(options: SimulatedOptions = {}) {
    this.now = options.now ?? Date.now;
    this.retryDelayMs = options.retryDelayMs ?? 1500;
    this.latencyMs = options.latencyMs ?? 0;
    this.agents = (options.agents ?? []).map((agent) => ({ ...agent }));
    this.servers = (options.servers ?? SAMPLE_SERVERS).map((server) => ({ ...server }));
    for (const server of this.servers) {
      this.events.set(server.id, this.connectedEvent(server.id));
      const agent = this.agentAt(server.host, server.port);
      this.pins.set(server.id, agent?.fingerprint ?? SAMPLE_FINGERPRINT);
    }
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
    const unread = this.unreadOperations;
    this.unreadOperations = [];
    for (const event of unread) listener({ ...event });
    return () => void this.operationListeners.delete(listener);
  }

  async onFingerprintChanged(listener: (change: FingerprintChange) => void): Promise<Unsubscribe> {
    this.fingerprintListeners.add(listener);
    for (const change of this.alerts.values()) listener({ ...change });
    return () => void this.fingerprintListeners.delete(listener);
  }

  async onNotice(listener: (notice: LinkNotice) => void): Promise<Unsubscribe> {
    this.noticeListeners.add(listener);
    const unread = this.unreadNotices;
    this.unreadNotices = [];
    for (const notice of unread) listener({ ...notice });
    return () => void this.noticeListeners.delete(listener);
  }

  // --- Commandes ---

  async probeServer(host: string, port: number | null): Promise<ProbeResult> {
    this.calls.push(`probe ${host}:${port ?? DEFAULT_PORT}`);
    await this.delay();
    const agent = this.agentAt(host, port ?? DEFAULT_PORT);
    if (!agent) throw this.fail({ kind: "unreachable" });
    if (agent.incompatible === "agent") throw this.fail({ kind: "incompatible_agent" });
    if (agent.incompatible === "client") throw this.fail({ kind: "incompatible_client" });
    return {
      fingerprint: agent.fingerprint,
      display: groupFingerprint(agent.fingerprint),
      machineName: agent.machineName ?? "machine",
      agentVersion: "0.1.0",
      macAddresses: [],
    };
  }

  async addAndLogin(input: NewServerInput): Promise<ServerInfo> {
    this.calls.push(`add-and-login ${input.name} remember=${input.remember}`);
    await this.delay();
    const port = input.port ?? DEFAULT_PORT;
    if (
      this.servers.some((s) => s.host.toLowerCase() === input.host.toLowerCase() && s.port === port)
    ) {
      throw this.fail({ kind: "already_exists" });
    }
    if (this.servers.some((s) => s.name.trim().toLowerCase() === input.name.trim().toLowerCase())) {
      throw this.fail({ kind: "name_taken" });
    }
    const agent = this.agentAt(input.host, port);
    if (!agent) throw this.fail({ kind: "unreachable" });
    // Contact épinglé sur l'empreinte confirmée : une autre identité est refusée avant tout identifiant.
    if (agent.fingerprint !== input.fingerprint) throw this.fail({ kind: "fingerprint_changed" });
    const key = `${input.host.toLowerCase()}:${port}`;
    const failed = this.failedLogins.get(key) ?? 0;
    if (failed >= MAX_FAILED_LOGINS) {
      throw this.fail({ kind: "too_many_attempts", retry_after_s: THROTTLE_SECONDS });
    }
    const user = agent.users?.[input.username.trim()];
    if (!user || user.password !== input.password) {
      this.failedLogins.set(key, failed + 1);
      throw this.fail({ kind: "invalid_credentials" });
    }
    this.failedLogins.set(key, 0);
    // La connexion a réussi : seulement maintenant le serveur, son empreinte et ses secrets existent.
    const server: ServerInfo = {
      id: `sim-${this.nextServer++}`,
      name: input.name.trim(),
      address: port === DEFAULT_PORT ? input.host : `${input.host}:${port}`,
      host: input.host,
      port,
      color: input.color,
      role: user.role,
      username: input.username.trim(),
      remember: input.remember,
    };
    this.pins.set(server.id, input.fingerprint);
    if (input.remember) this.vault.set(server.id, input.password);
    this.servers = [...this.servers, server];
    this.emitServers();
    this.publish(server.id, "connected");
    return { ...server };
  }

  async login(
    serverId: string,
    username: string,
    password: string,
    remember: boolean,
  ): Promise<{ role: Role }> {
    this.calls.push(`login ${serverId} ${username} remember=${remember}`);
    await this.delay();
    const server = this.requireServer(serverId);
    const agent = this.agentAt(server.host, server.port);
    if (!agent) throw this.fail({ kind: "unreachable" });
    if (agent.fingerprint !== this.pins.get(serverId)) {
      throw this.fail({ kind: "fingerprint_changed" });
    }
    const failed = this.failedLogins.get(serverId) ?? 0;
    if (failed >= MAX_FAILED_LOGINS) {
      throw this.fail({ kind: "too_many_attempts", retry_after_s: THROTTLE_SECONDS });
    }
    const user = agent.users?.[username.trim()];
    if (!user || user.password !== password) {
      this.failedLogins.set(serverId, failed + 1);
      throw this.fail({ kind: "invalid_credentials" });
    }
    this.failedLogins.set(serverId, 0);
    if (remember) this.vault.set(serverId, password);
    else this.vault.delete(serverId);
    this.replaceServer({ ...server, username: username.trim(), remember, role: user.role });
    this.publish(serverId, "connected");
    return { role: user.role };
  }

  async logout(serverId: string): Promise<void> {
    this.calls.push(`logout ${serverId}`);
    this.requireServer(serverId);
    this.publish(serverId, "session_expired", { reason: "user_disconnected" });
  }

  async acceptFingerprint(serverId: string, fingerprint: string): Promise<void> {
    this.calls.push(`accept ${serverId}`);
    const server = this.requireServer(serverId);
    const alert = this.alerts.get(serverId);
    if (!alert || alert.presentedHex !== fingerprint) {
      throw this.fail({ kind: "verification_required" });
    }
    this.pins.set(serverId, alert.presentedHex);
    this.alerts.delete(serverId);
    const agent = this.agentAt(server.host, server.port);
    if (agent && agent.fingerprint === alert.presentedHex) this.publish(serverId, "connected");
  }

  async updateServer(serverId: string, edit: ServerEdit): Promise<ServerInfo> {
    this.calls.push(`update ${serverId}`);
    await this.delay();
    const server = this.requireServer(serverId);
    const port = edit.port ?? DEFAULT_PORT;
    const others = this.servers.filter((s) => s.id !== serverId);
    if (others.some((s) => s.host.toLowerCase() === edit.host.toLowerCase() && s.port === port)) {
      throw this.fail({ kind: "already_exists" });
    }
    if (others.some((s) => s.name.trim().toLowerCase() === edit.name.trim().toLowerCase())) {
      throw this.fail({ kind: "name_taken" });
    }
    const moved = server.host.toLowerCase() !== edit.host.toLowerCase() || server.port !== port;
    if (moved) {
      if (edit.fingerprint === null) throw this.fail({ kind: "verification_required" });
      this.pins.set(serverId, edit.fingerprint);
    }
    const updated: ServerInfo = {
      ...server,
      name: edit.name.trim(),
      color: edit.color,
      host: edit.host,
      port,
      address: port === DEFAULT_PORT ? edit.host : `${edit.host}:${port}`,
    };
    this.replaceServer(updated);
    if (moved) {
      const agent = this.agentAt(updated.host, updated.port);
      const ok = agent?.fingerprint === this.pins.get(serverId);
      this.publish(serverId, ok ? "connected" : "offline");
    }
    return { ...updated };
  }

  async removeServer(serverId: string): Promise<void> {
    this.calls.push(`remove ${serverId}`);
    this.requireServer(serverId);
    this.dropServer(serverId);
    this.vault.delete(serverId);
    this.pins.delete(serverId);
    this.alerts.delete(serverId);
  }

  async forgetCredentials(serverId: string): Promise<void> {
    this.calls.push(`forget ${serverId}`);
    const server = this.requireServer(serverId);
    this.vault.delete(serverId);
    this.replaceServer({ ...server, remember: false });
  }

  async runAction(serverId: string, action: ActionInput): Promise<ActionResult> {
    this.calls.push(`action ${action.method} ${action.path}`);
    this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    if (this.actionMode === "cut") {
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    return { kind: "completed", status: 200, body: "{}" };
  }

  async setDisplayedServer(serverId: string | null): Promise<void> {
    this.displayedServer = serverId;
  }

  // --- Pilotage (code de test, panneau de développement) ---

  /** Fait passer un serveur dans `state` et émet l'événement correspondant. */
  setState(serverId: string, state: LinkState): void {
    this.publish(serverId, state);
  }

  /** Comme `setState`, avec la raison d'arrêt ou le blocage. */
  publish(
    serverId: string,
    state: LinkState,
    extra: Partial<Pick<LinkStateEvent, "blocked" | "reason" | "failedAttempts">> = {},
  ): void {
    const previous = this.events.get(serverId);
    const now = this.now();
    const event: LinkStateEvent = {
      serverId,
      seq: this.nextSeq(serverId),
      state,
      since: now,
      lastContactAt: state === "connected" ? now : (previous?.lastContactAt ?? null),
      nextRetryAt: state === "reconnecting" || state === "offline" ? now + 5000 : null,
      blocked: extra.blocked ?? null,
      reason: extra.reason ?? null,
      failedAttempts: extra.failedAttempts ?? 0,
    };
    this.events.set(serverId, event);
    if (event.blocked !== "fingerprint_changed") this.alerts.delete(serverId);
    for (const listener of [...this.stateListeners]) listener({ ...event });
  }

  /** Émet une issue d'opération. */
  emitOperation(event: OperationEvent): void {
    if (this.operationListeners.size === 0) this.unreadOperations.push({ ...event });
    for (const listener of [...this.operationListeners]) listener({ ...event });
  }

  /** Émet un avis de la liaison. */
  emitNotice(notice: Omit<LinkNotice, "id"> & { id?: number }): void {
    const full: LinkNotice = { ...notice, id: notice.id ?? ++this.noticeIds };
    if (this.noticeListeners.size === 0) this.unreadNotices.push({ ...full });
    for (const listener of [...this.noticeListeners]) listener({ ...full });
  }

  /** Ajoute un agent au réseau simulé. */
  addAgent(agent: SimAgent): void {
    this.agents.push({ ...agent });
  }

  /**
   * Le serveur est réinstallé : l'agent présente une autre empreinte. Si on y était connecté, le
   * lien tombe et l'alerte d'empreinte change (BR-CONN-003).
   */
  reinstallAgent(serverId: string, newFingerprint: string): void {
    const server = this.requireServer(serverId);
    const agent = this.agentAt(server.host, server.port);
    const expected = this.pins.get(serverId) ?? SAMPLE_FINGERPRINT;
    if (agent) agent.fingerprint = newFingerprint;
    this.publish(serverId, "offline", { blocked: "fingerprint_changed" });
    const change: FingerprintChange = {
      serverId,
      expected: groupFingerprint(expected),
      presented: groupFingerprint(newFingerprint),
      presentedHex: newFingerprint,
    };
    this.alerts.set(serverId, change);
    for (const listener of [...this.fingerprintListeners]) listener({ ...change });
  }

  /** Ajoute un serveur directement au carnet, connecté (amorçage des tests). */
  seedServer(server: ServerInfo): void {
    this.servers = [...this.servers, { ...server }];
    const event = this.connectedEvent(server.id);
    this.events.set(server.id, event);
    this.pins.set(
      server.id,
      this.agentAt(server.host, server.port)?.fingerprint ?? SAMPLE_FINGERPRINT,
    );
    this.emitServers();
    for (const listener of [...this.stateListeners]) listener({ ...event });
  }

  /** Retire un serveur du carnet (amorçage des tests). */
  dropServer(serverId: string): void {
    this.servers = this.servers.filter((server) => server.id !== serverId);
    this.events.delete(serverId);
    this.emitServers();
  }

  private agentAt(host: string, port: number): SimAgent | undefined {
    return this.agents.find(
      (agent) =>
        agent.host.toLowerCase() === host.trim().toLowerCase() &&
        (agent.port ?? DEFAULT_PORT) === port,
    );
  }

  private requireServer(serverId: string): ServerInfo {
    const server = this.servers.find((s) => s.id === serverId);
    if (!server) throw this.fail({ kind: "unknown_server" });
    return server;
  }

  private replaceServer(server: ServerInfo): void {
    this.servers = this.servers.map((s) => (s.id === server.id ? { ...server } : s));
    this.emitServers();
  }

  private fail(failure: LinkFailure): LinkCommandError {
    return new LinkCommandError(failure);
  }

  private async delay(): Promise<void> {
    if (this.latencyMs > 0) await new Promise((resolve) => setTimeout(resolve, this.latencyMs));
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
      blocked: null,
      reason: null,
      failedAttempts: 0,
    };
  }

  private nextSeq(serverId: string): number {
    const next = (this.seqs.get(serverId) ?? 0) + 1;
    this.seqs.set(serverId, next);
    return next;
  }
}
