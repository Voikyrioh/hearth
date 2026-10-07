import type { AgentUpdateEvent, AgentUpdateOutcome, AgentUpdateView } from "./agent-update";
import type { AuditEntry, AuditExportResult, AuditFilter, AuditPage } from "./audit";
import type { LinkBridge } from "./bridge";
import type { DeviceRemovalOutcome, TrustedDevices } from "./devices";
import type { MachineEvent } from "./machine";
import type { AdminActKind, ReauthMode, ReauthSettingOutcome, ReauthState } from "./reauth";
import type { AttackModeOutcome, SecurityRead, SecurityState } from "./security";
import {
  type SimAccountResult,
  SimulatedAccounts,
  simulatedCheckInput,
} from "./simulated-accounts";
import { SimulatedAgentUpdates } from "./simulated-agent-update";
import { SimulatedAudit } from "./simulated-audit";
import { SimulatedDevices } from "./simulated-devices";
import { bareMachine, SimulatedMachine } from "./simulated-machine";
import { SimulatedReauth, simulatedCovers } from "./simulated-reauth";
import { SimulatedSecurity } from "./simulated-security";
import {
  type AccountInputCheck,
  type AccountList,
  type AccountOutcome,
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
  /**
   * Mesures en direct : un échantillon par seconde et cinq minutes d'historique déjà là
   * (navigateur de développement). Sans cela, aucune mesure ne part seule : les tests pilotent
   * `bridge.machine` à la main.
   */
  liveMetrics?: boolean;
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
  /** La case « Garder ce poste reconnu » de la dernière demande de changement de mot de passe (observable dans les tests). */
  lastKeepAddress: boolean | null = null;
  /** Dernière clé d'opération donnée à une action restée sans réponse (observable dans les tests). */
  lastUnknownOpId: string | null = null;
  /** Serveur affiché dans la fenêtre, tel que la coquille l'a reçu (observable dans les tests). */
  displayedServer: string | null = null;
  private nextOperation = 1;
  /** Les machines simulées : mesures plausibles, niveaux pilotables (tableau de bord). */
  readonly machine: SimulatedMachine;
  /** Les comptes simulés de chaque serveur (HRT-13) : amorçage des tests (`accounts.seed`). */
  readonly accounts: SimulatedAccounts;
  /** Les postes de confiance simulés de chaque serveur (HRT-23) : amorçage des tests (`devices.seed`). */
  readonly devices: SimulatedDevices;
  /** L'alerte et le mode attaque simulés (HRT-26) : pilotables par l'appelant (`security.setAlert`, `setMode`, `setDevice`). */
  readonly security: SimulatedSecurity;
  /** La confirmation des actes simulée (HRT-30) : agent ancien, réglage, élévation, pilotables (`reauth.setSupported`, `setMode`, `open`). */
  readonly reauth: SimulatedReauth;
  /** Mot de passe actuel de l'utilisateur, par serveur (« Correct-Horse-9 » tant qu'il n'a pas changé). */
  private readonly ownPasswords = new Map<string, string>();
  /**
   * Une action de compte coupée (`actionMode = "cut"`) a-t-elle été exécutée par l'agent avant la
   * coupure ? Vrai : « fait pendant la coupure » ; faux : « non exécuté ». Dans les deux cas
   * l'interface reçoit « résultat inconnu ». Le mode revient à `ok` après une coupure.
   */
  executeBeforeCut = true;
  /** Le journal d'activité simulé (filtre, pagination, direct : comme l'agent). */
  readonly audit: SimulatedAudit;
  /** La mise à jour de l'agent simulée (HRT-17) : état par serveur, étapes pilotables par le test. */
  readonly agentUpdates: SimulatedAgentUpdates;

  constructor(options: SimulatedOptions = {}) {
    this.now = options.now ?? Date.now;
    this.retryDelayMs = options.retryDelayMs ?? 1500;
    this.latencyMs = options.latencyMs ?? 0;
    this.accounts = new SimulatedAccounts(this.now);
    this.devices = new SimulatedDevices(this.now);
    this.security = new SimulatedSecurity(this.now);
    this.reauth = new SimulatedReauth(this.now, (id) => this.security.current(id).keyAtHand);
    this.agents = (options.agents ?? []).map((agent) => ({ ...agent }));
    this.servers = (options.servers ?? SAMPLE_SERVERS).map((server) => ({ ...server }));
    this.machine = new SimulatedMachine({
      now: this.now,
      connected: (id) => this.events.get(id)?.state === "connected",
      // Le serveur « salon » est un boîtier sans carte graphique ni sonde : de quoi voir les états vides.
      machines: { salon: bareMachine("nas-salon") },
    });
    this.agentUpdates = new SimulatedAgentUpdates({
      now: this.now,
      publishLink: (id, state) => this.publish(id, state),
    });
    this.audit = new SimulatedAudit({
      now: this.now,
      connected: (id) => this.events.get(id)?.state === "connected",
      role: (id) => this.servers.find((server) => server.id === id)?.role ?? "readonly",
    });
    if (options.liveMetrics) {
      this.audit.seed("forge", 250);
      for (const server of this.servers) this.machine.prefill(server.id, 300);
      this.machine.start();
    }
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

  async onMachine(serverId: string, listener: (event: MachineEvent) => void): Promise<Unsubscribe> {
    return this.machine.subscribe(serverId, listener);
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

  /**
   * Action d'essai du navigateur de développement (panneau `DevActionPanel`) : n'existe que dans le
   * pont simulé, jamais dans le pont réel ni dans le binaire livré.
   */
  async runDevAction(serverId: string): Promise<ActionResult> {
    this.calls.push("action dev-ping");
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

  // --- Comptes (HRT-13) : mêmes règles que l'agent ; `calls` ne contient jamais un mot de passe ---

  async checkAccountInput(username: string, password: string): Promise<AccountInputCheck> {
    return simulatedCheckInput(username, password);
  }

  async listAccounts(serverId: string): Promise<AccountList> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push("account list");
    return { accounts: this.accounts.list(server), me: this.accounts.meId(server) };
  }

  createAccount(
    serverId: string,
    username: string,
    password: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return this.accountAction(
      serverId,
      `create ${username}`,
      (server) => this.accounts.create(server, username, password, role),
      { kind: "account_create", role, password: adminPassword },
    );
  }

  changeAccountRole(
    serverId: string,
    accountId: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return this.accountAction(
      serverId,
      `role ${accountId} ${role}`,
      (server) => this.accounts.changeRole(server, accountId, role),
      { kind: "account_role", role, password: adminPassword },
    );
  }

  setAccountPassword(
    serverId: string,
    accountId: string,
    password: string,
    adminPassword: string,
  ): Promise<AccountOutcome> {
    return this.accountAction(
      serverId,
      `password ${accountId}`,
      (server) => this.accounts.setPassword(server, accountId, password),
      { kind: "account_password", role: null, password: adminPassword },
    );
  }

  changeOwnPassword(
    serverId: string,
    current: string,
    password: string,
    keepAddress: boolean,
  ): Promise<AccountOutcome> {
    this.lastKeepAddress = keepAddress;
    return this.accountAction(
      serverId,
      "own-password",
      (server) => {
        const right = current === this.own(serverId);
        const result = this.accounts.changeOwn(server, right, password);
        if (result.outcome.kind === "done") {
          this.ownPasswords.set(serverId, password);
          // Changer son mot de passe ferme l'élévation (BR-TRUST-043).
          this.reauth.close(serverId);
        }
        return result;
      },
      // L'ancien mot de passe EST le mot de passe de confirmation de l'acte.
      { kind: "account_password_own", role: null, password: current },
    );
  }

  closeAccountSessions(
    serverId: string,
    accountId: string,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return this.accountAction(
      serverId,
      `sessions ${accountId}`,
      (server) => this.accounts.closeSessions(server, accountId),
      { kind: "sessions_revoke", role: null, password: adminPassword },
    );
  }

  deleteAccount(
    serverId: string,
    accountId: string,
    confirmation: string | null,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return this.accountAction(
      serverId,
      `delete ${accountId}`,
      (server) => this.accounts.delete(server, accountId, confirmation),
      { kind: "account_delete", role: null, password: adminPassword },
    );
  }

  /** Mot de passe actuel de l'utilisateur de ce serveur (celui que la confirmation compare). */
  private own(serverId: string): string {
    return this.ownPasswords.get(serverId) ?? "Correct-Horse-9";
  }

  // --- Confirmation des actes (HRT-30) ---

  async getReauthState(serverId: string): Promise<ReauthState> {
    this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push("reauth read");
    return this.reauth.state(serverId);
  }

  async reauthCovers(kind: AdminActKind, role: Role | null): Promise<boolean> {
    return simulatedCovers(kind, role);
  }

  async setReauthSetting(
    serverId: string,
    mode: ReauthMode,
    password: string,
  ): Promise<ReauthSettingOutcome> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    const confirmation = this.reauth.confirm(
      serverId,
      "reauth_setting",
      server.role,
      password,
      this.own(serverId),
    );
    this.calls.push(`reauth setting ${mode}`);
    if (confirmation.kind === "refused") return confirmation;
    if (this.actionMode === "cut") {
      this.actionMode = "ok";
      if (this.executeBeforeCut) this.reauth.setMode(serverId, mode);
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    this.reauth.setMode(serverId, mode);
    return { kind: "done", mode };
  }

  /** Toute action de compte : hors « Connecté » rien ne part ; coupée avant la réponse, jamais rejouée. */
  private async accountAction(
    serverId: string,
    label: string,
    run: (server: ServerInfo) => SimAccountResult,
    confirm: { kind: AdminActKind; role: Role | null; password: string | null },
  ): Promise<AccountOutcome> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    // Sans clé au coffre la coquille ne part pas : rien n'est noté. Puis le mot de passe, comme l'agent.
    const confirmation = this.reauth.confirm(
      serverId,
      confirm.kind,
      confirm.role,
      confirm.password,
      this.own(serverId),
    );
    // Comme le vrai pont : hors « Connecté » rien ne part, donc rien n'est noté.
    this.calls.push(`account ${label}`);
    if (confirmation.kind === "refused") return confirmation;
    if (this.actionMode === "cut") {
      this.actionMode = "ok";
      if (this.executeBeforeCut) this.settle(serverId, run(server));
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    return this.settle(serverId, run(server));
  }

  /** Sa propre session fermée par l'action (mot de passe, suppression) : « Accès révoqué ». */
  private settle(serverId: string, result: SimAccountResult): AccountOutcome {
    if (result.ended) this.publish(serverId, "access_revoked", { reason: "revoked" });
    return result.outcome;
  }

  // --- Postes de confiance (HRT-23) : `calls` ne contient jamais un mot de passe ---

  async listTrustedDevices(serverId: string): Promise<TrustedDevices> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push("devices list");
    return this.devices.list(server);
  }

  async removeTrustedDevice(
    serverId: string,
    deviceId: string,
    password: string,
  ): Promise<DeviceRemovalOutcome> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push(`devices remove ${deviceId}`);
    const own = this.own(serverId);
    if (this.actionMode === "cut") {
      // Comme une action de compte : coupée avant la réponse, jamais rejouée.
      this.actionMode = "ok";
      if (this.executeBeforeCut) this.devices.remove(server, deviceId, password, own);
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    return this.devices.remove(server, deviceId, password, own);
  }

  // --- Sécurité (HRT-26) : `calls` ne contient jamais un mot de passe ---

  async onSecurity(listener: (state: SecurityState) => void): Promise<Unsubscribe> {
    return this.security.subscribe(listener);
  }

  async getSecurity(serverId: string): Promise<SecurityRead> {
    this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push("security read");
    return this.security.read(serverId);
  }

  async setAttackMode(
    serverId: string,
    active: boolean,
    password: string,
  ): Promise<AttackModeOutcome> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    const own = this.own(serverId);
    // La confirmation d'abord (sans clé : la coquille ne part pas, `not_recognized`), jamais couverte.
    if (this.security.current(serverId).keyAtHand) {
      if (server.role !== "admin") throw this.fail({ kind: "forbidden" });
      const confirmation = this.reauth.confirm(
        serverId,
        active ? "attack_mode_enable" : "attack_mode_disable",
        server.role,
        password,
        own,
      );
      if (confirmation.kind === "refused") {
        this.calls.push(`attack-mode ${active ? "on" : "off"}`);
        return confirmation;
      }
    }
    if (this.actionMode === "cut") {
      // Comme une action de compte : coupée avant la réponse, jamais rejouée.
      this.actionMode = "ok";
      this.calls.push(`attack-mode ${active ? "on" : "off"}`);
      if (this.executeBeforeCut) {
        try {
          this.security.change(serverId, active, password, own, server.role);
        } catch {
          // Refusé avant d'avoir été exécuté : la coupure ne change rien.
        }
      }
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    // Sans clé au coffre rien ne part : aucune ligne dans `calls` (la commande échoue avant l'envoi).
    const outcome = this.security.change(serverId, active, password, own, server.role);
    this.calls.push(`attack-mode ${active ? "on" : "off"}`);
    // Activer ferme toutes les élévations (BR-TRUST-043).
    if (active && outcome.kind === "done") this.reauth.close(serverId);
    return outcome;
  }

  // --- Mise à jour de l'agent (HRT-17) ---

  async getAgentUpdate(serverId: string): Promise<AgentUpdateView> {
    this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    this.calls.push("agent-update read");
    return this.agentUpdates.view(serverId);
  }

  async ackAgentResult(serverId: string, at: string): Promise<void> {
    this.requireServer(serverId);
    this.calls.push(`agent-update ack ${at}`);
    this.agentUpdates.acked.set(serverId, at);
  }

  async updateAgent(
    serverId: string,
    version: string,
    adminPassword: string,
  ): Promise<AgentUpdateOutcome> {
    const server = this.requireServer(serverId);
    await this.delay();
    if (this.events.get(serverId)?.state !== "connected") {
      throw this.fail({ kind: "not_connected" });
    }
    // Comme le vrai pont : hors « Connecté » rien ne part, donc rien n'est noté. La cible n'est
    // jamais fournie par l'interface : seulement le numéro de version que l'utilisateur a vu.
    this.calls.push(`agent-update ${version}`);
    // L'agent est l'arbitre du rôle : un compte Lecture seule est refusé chez lui.
    if (server.role !== "admin") throw this.fail({ kind: "forbidden" });
    const confirmation = this.reauth.confirm(
      serverId,
      "agent_update",
      server.role,
      adminPassword,
      this.own(serverId),
    );
    if (confirmation.kind === "refused") return confirmation;
    if (this.actionMode === "cut") {
      this.actionMode = "ok";
      if (this.executeBeforeCut) this.agentUpdates.start(serverId, version);
      this.publish(serverId, "reconnecting");
      const opId = `sim-op-${this.nextOperation++}`;
      this.lastUnknownOpId = opId;
      return { kind: "unknown", opId };
    }
    return this.agentUpdates.start(serverId, version);
  }

  async onAgentUpdate(listener: (event: AgentUpdateEvent) => void): Promise<Unsubscribe> {
    return this.agentUpdates.subscribe(listener);
  }

  async readAudit(
    serverId: string,
    filter: AuditFilter,
    before: number | null,
  ): Promise<AuditPage> {
    this.calls.push(`readAudit ${serverId}`);
    return this.audit.read(serverId, filter, before);
  }

  async exportAudit(serverId: string, filter: AuditFilter): Promise<AuditExportResult> {
    this.calls.push(`exportAudit ${serverId}`);
    return this.audit.export(serverId, filter);
  }

  async onAudit(
    serverId: string,
    listener: (entry: AuditEntry) => void,
    onGap: () => void = () => {},
  ): Promise<Unsubscribe> {
    return this.audit.subscribe(serverId, listener, onGap);
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
    // Le lien est revenu : l'agent renvoie son identité et son historique (BR-DASH-011).
    if (state === "connected" && previous && previous.state !== "connected") {
      this.machine.resync(serverId);
    }
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
