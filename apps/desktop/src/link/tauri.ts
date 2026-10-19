import { listen } from "@tauri-apps/api/event";
import {
  type AccountListDto,
  type AuditLiveEvent,
  type AgentUpdateEvent as BoundAgentUpdateEvent,
  type LinkFailure as BoundFailure,
  type AccountOutcome as BoundOutcome,
  type ServiceOperationEvent as BoundServiceOperationEvent,
  type ServicesEvent as BoundServicesEvent,
  commands,
  type FingerprintEvent,
  type HistoryEvent,
  type LinkStateDto,
  type MetricsEvent,
  type NoticeEvent,
  type OperationEventDto,
  type SecurityEvent,
  type ServerDto,
  type ServersEvent,
  type ServiceLogEvent,
  type SnapshotEvent,
} from "@/bindings";
import {
  type AgentUpdateEvent,
  type AgentUpdateOutcome,
  type AgentUpdateView,
  toAgentUpdateOutcome,
} from "./agent-update";
import type { AuditEntry, AuditExportResult, AuditFilter, AuditPage } from "./audit";
import { toAuditFilterDto, toAuditLive, toAuditPage } from "./audit";
import type { LinkBridge } from "./bridge";
import {
  type DeviceRemovalOutcome,
  type TrustedDevices,
  toDeviceRemovalOutcome,
  toTrustedDevices,
} from "./devices";
import { type LogMessage, type LogReading, toLogMessage } from "./logs";
import { type MachineEvent, toHistory, toMetrics, toView } from "./machine";
import {
  type AdminActKind,
  type ReauthMode,
  type ReauthSettingOutcome,
  type ReauthState,
  toReauthSettingOutcome,
} from "./reauth";
import {
  type AttackModeOutcome,
  type SecurityRead,
  type SecurityState,
  toAttackModeOutcome,
  toSecurityRead,
  toSecurityState,
} from "./security";
import {
  type LifecycleGesture,
  type LifecycleOutcome,
  toLifecycleOutcome,
} from "./serviceLifecycle";
import {
  type DecisionOutcome,
  type OperationChoice,
  type ServiceOperationEvent,
  toDecisionOutcome,
  toServiceOperationEvent,
} from "./serviceOps";
import {
  type EngineInfo,
  type EnvVariable,
  type ServiceRights,
  type ServicesEvent,
  toEngine,
  toEnvVariables,
  toServiceRights,
  toServicesEvent,
} from "./services";
import {
  type AccountInputCheck,
  type AccountList,
  type AccountOutcome,
  type FingerprintChange,
  LinkCommandError,
  type LinkNotice,
  type LinkStateEvent,
  type NewServerInput,
  type OperationEvent,
  type ProbeResult,
  type Role,
  SERVER_COLORS,
  type ServerColor,
  type ServerEdit,
  type ServerInfo,
  type Unsubscribe,
} from "./types";

/** Noms des événements de la coquille (conception technique, section 9). */
/** Période du battement de vie des lectures de journaux (`KEEP_ALIVE_PERIOD` de la coquille : dix secondes). */
export const KEEP_ALIVE_MS = 10_000;

export const LINK_EVENTS = {
  servers: "link://servers",
  state: "link://state",
  operation: "link://operation",
  fingerprint: "link://fingerprint",
  notice: "link://notice",
  snapshot: "link://snapshot",
  metrics: "link://metrics",
  history: "link://history",
  audit: "link://audit",
  auditGap: "link://audit-gap",
  agentUpdate: "agent-update://progress",
  security: "link://security",
  services: "link://services",
  serviceLogs: "link://service-logs",
  serviceOperations: "link://service-operations",
} as const;

function toColor(value: number): ServerColor {
  return SERVER_COLORS.find((color) => color === value) ?? 1;
}

/** Un serveur tel que la coquille l'envoie, avec une couleur de la palette. */
export function toServerInfo(dto: ServerDto): ServerInfo {
  return {
    id: dto.id,
    name: dto.name,
    address: dto.address,
    host: dto.host,
    port: dto.port,
    color: toColor(dto.color),
    role: dto.role,
    username: dto.username,
    remember: dto.remember,
  };
}

export function toStateEvent(dto: LinkStateDto, now: () => number = Date.now): LinkStateEvent {
  return {
    serverId: dto.serverId,
    seq: dto.seq,
    state: dto.state,
    since: dto.since ?? now(),
    lastContactAt: dto.lastContactAt,
    nextRetryAt: dto.nextRetryAt,
    blocked: dto.blocked,
    reason: dto.reason,
    failedAttempts: dto.failedAttempts,
  };
}

/** L'issue d'une action de compte, avec les noms de champs de l'interface. */
export function toAccountOutcome(dto: BoundOutcome): AccountOutcome {
  switch (dto.kind) {
    case "done":
      return { kind: "done", account: dto.account, sessionsClosed: dto.sessions_closed };
    case "refused":
      return dto;
    case "unknown":
      return { kind: "unknown", opId: dto.op_id };
  }
}

export function toAccountList(dto: AccountListDto): AccountList {
  return dto;
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: BoundFailure };

/** Rend la donnée d'une commande, ou rejette avec l'échec typé. */
function unwrap<T>(result: Result<T>): T {
  if (result.status === "ok") return result.data;
  throw new LinkCommandError(result.error);
}

/**
 * Pont réel : commandes et événements `link://*` de la coquille Tauri, qui les tient de
 * `hearth-link`. Aucune règle ici : l'interface ne parle aux serveurs que par ce pont.
 * Les abonnements s'abonnent D'ABORD aux événements, puis lisent l'instantané (rejeu).
 */
export class TauriLinkBridge implements LinkBridge {
  private readonly now: () => number;
  private readonly keepAliveMs: number;
  /** La génération de cette page auprès de la coquille (journaux) : annoncée une fois, au démarrage de la page. */
  private logPage: Promise<number> | null = null;
  /** Les lectures de journaux ouvertes, avec ce qu'il faut faire si la coquille ne les tient plus. */
  private readonly readings = new Map<string, () => void>();
  private keepTimer: ReturnType<typeof setInterval> | undefined;

  constructor(now: () => number = Date.now, keepAliveMs: number = KEEP_ALIVE_MS) {
    this.now = now;
    this.keepAliveMs = keepAliveMs;
  }

  /**
   * Annonce cette page à la coquille et rend sa génération. La coquille ferme alors tout ce qu'une page précédente
   * de cette fenêtre (rechargée, plantée) avait laissé ouvert : l'interface n'a pas à le faire (ADR-0040).
   */
  announceLogPage(): Promise<number> {
    if (this.logPage === null) {
      this.logPage = commands.announceLogPage().catch((error: unknown) => {
        this.logPage = null;
        throw error;
      });
    }
    return this.logPage;
  }

  private stopKeepAlive(): void {
    if (this.keepTimer !== undefined && this.readings.size === 0) {
      clearInterval(this.keepTimer);
      this.keepTimer = undefined;
    }
  }

  /** Un battement : la coquille rend les sessions qu'elle tient encore ; celles qu'elle a fermées sont dites perdues. */
  private async beat(): Promise<void> {
    const asked = [...this.readings.keys()];
    if (asked.length === 0) return;
    try {
      const alive = new Set(await commands.keepServiceLogs(await this.announceLogPage()));
      for (const id of asked) if (!alive.has(id)) this.readings.get(id)?.();
    } catch {
      // Un battement raté n'est pas une perte : le suivant réessaie, et la coquille ferme seule au bout du délai.
    }
  }

  async onServersChanged(listener: (servers: ServerInfo[]) => void): Promise<Unsubscribe> {
    let newer = false;
    const unlisten = await listen<ServersEvent>(LINK_EVENTS.servers, (event) => {
      newer = true;
      listener(event.payload.servers.map(toServerInfo));
    });
    try {
      const list = await commands.listServers();
      // Un événement arrivé pendant la lecture est au moins aussi récent : il l'emporte.
      if (!newer) listener(list.map(toServerInfo));
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async onLinkState(listener: (event: LinkStateEvent) => void): Promise<Unsubscribe> {
    const unlisten = await listen<LinkStateDto>(LINK_EVENTS.state, (event) =>
      listener(toStateEvent(event.payload, this.now)),
    );
    try {
      for (const state of await commands.listLinkStates()) {
        listener(toStateEvent(state, this.now));
      }
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async onMachine(serverId: string, listener: (event: MachineEvent) => void): Promise<Unsubscribe> {
    const unlisteners: Unsubscribe[] = [];
    const release = () => {
      for (const unlisten of unlisteners) unlisten();
    };
    try {
      unlisteners.push(
        await listen<SnapshotEvent>(LINK_EVENTS.snapshot, (event) => {
          if (event.payload.serverId !== serverId) return;
          const view = toView(event.payload);
          if (view) listener({ kind: "view", view });
        }),
      );
      unlisteners.push(
        await listen<MetricsEvent>(LINK_EVENTS.metrics, (event) => {
          if (event.payload.serverId !== serverId) return;
          const metrics = toMetrics(event.payload);
          if (metrics) listener({ kind: "metrics", metrics });
        }),
      );
      unlisteners.push(
        await listen<HistoryEvent>(LINK_EVENTS.history, (event) => {
          if (event.payload.serverId !== serverId) return;
          const history = toHistory(event.payload);
          if (history.length > 0) listener({ kind: "history", history });
        }),
      );
      // Écoute posée d'abord, lecture ensuite. Rien n'est jeté : la vue lue complète ce que le flux
      // a déjà apporté (le récepteur recolle par instant), et l'identité de la machine arrive
      // toujours, même si des échantillons l'ont précédée.
      const last = unwrap(await commands.getDashboard(serverId));
      const view = last ? toView(last) : null;
      if (view) listener({ kind: "view", view });
    } catch (error) {
      release();
      throw error;
    }
    return release;
  }

  async retryNow(serverId: string): Promise<void> {
    unwrap(await commands.retryNow(serverId));
  }

  /**
   * S'abonne d'abord, puis rejoue ce que la coquille a retenu : un événement n'est qu'un signal, il
   * peut être parti avant que cette interface n'écoute (lancement de l'application). Chaque avis
   * ou issue reçu, en direct ou rejoué, est acquitté après avoir été remis à l'écouteur.
   */
  private async subscribeAndReplay<T>(
    name: string,
    listener: (payload: T) => void,
    replay: () => Promise<T[]>,
    acknowledge?: (payload: T) => Promise<unknown>,
  ): Promise<Unsubscribe> {
    const deliver = (payload: T) => {
      listener(payload);
      // L'acquittement est au mieux : une perte ne fait que rejouer (l'écouteur dédoublonne).
      void acknowledge?.(payload)?.catch(() => {});
    };
    const unlisten = await listen<T>(name, (event) => deliver(event.payload));
    try {
      for (const payload of await replay()) deliver(payload);
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async onOperation(listener: (event: OperationEvent) => void): Promise<Unsubscribe> {
    return this.subscribeAndReplay<OperationEventDto>(
      LINK_EVENTS.operation,
      listener,
      () => commands.listUnreadOperations(),
      (event) => commands.ackUnreadOperations([event.opId]),
    );
  }

  async onFingerprintChanged(listener: (change: FingerprintChange) => void): Promise<Unsubscribe> {
    // Une alerte est un état, pas un avis : rejouée à chaque abonnement, jamais acquittée.
    return this.subscribeAndReplay<FingerprintEvent>(LINK_EVENTS.fingerprint, listener, () =>
      commands.listFingerprintAlerts(),
    );
  }

  async onNotice(listener: (notice: LinkNotice) => void): Promise<Unsubscribe> {
    return this.subscribeAndReplay<NoticeEvent>(
      LINK_EVENTS.notice,
      listener,
      () => commands.listLinkNotices(),
      (notice) => (notice.id > 0 ? commands.ackLinkNotices([notice.id]) : Promise.resolve()),
    );
  }

  async probeServer(host: string, port: number | null): Promise<ProbeResult> {
    return unwrap(await commands.probeServer(host, port));
  }

  async addAndLogin(input: NewServerInput): Promise<ServerInfo> {
    return toServerInfo(unwrap(await commands.addAndLogin(input)));
  }

  async login(
    serverId: string,
    username: string,
    password: string,
    remember: boolean,
  ): Promise<{ role: Role }> {
    const login = unwrap(await commands.login(serverId, username, password, remember));
    return { role: login.role };
  }

  async logout(serverId: string): Promise<void> {
    unwrap(await commands.logout(serverId));
  }

  async acceptFingerprint(serverId: string, fingerprint: string): Promise<void> {
    unwrap(await commands.acceptFingerprint(serverId, fingerprint));
  }

  async updateServer(serverId: string, edit: ServerEdit): Promise<ServerInfo> {
    const dto = unwrap(
      await commands.updateServer(
        serverId,
        edit.name,
        edit.color,
        edit.host,
        edit.port,
        edit.fingerprint,
      ),
    );
    return toServerInfo(dto);
  }

  async removeServer(serverId: string): Promise<void> {
    unwrap(await commands.removeServer(serverId));
  }

  async forgetCredentials(serverId: string): Promise<void> {
    unwrap(await commands.forgetCredentials(serverId));
  }

  async setDisplayedServer(serverId: string | null): Promise<void> {
    await commands.setDisplayedServer(serverId);
  }

  async checkAccountInput(username: string, password: string): Promise<AccountInputCheck> {
    return commands.checkAccountInput(username, password);
  }

  async listAccounts(serverId: string): Promise<AccountList> {
    return toAccountList(unwrap(await commands.listAccounts(serverId)));
  }

  async createAccount(
    serverId: string,
    username: string,
    password: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.createAccount(serverId, username, password, role, adminPassword)),
    );
  }

  async changeAccountRole(
    serverId: string,
    accountId: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.changeAccountRole(serverId, accountId, role, adminPassword)),
    );
  }

  async setAccountPassword(
    serverId: string,
    accountId: string,
    password: string,
    adminPassword: string,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.setAccountPassword(serverId, accountId, password, adminPassword)),
    );
  }

  async changeOwnPassword(
    serverId: string,
    current: string,
    password: string,
    keepAddress: boolean,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.changeOwnPassword(serverId, current, password, keepAddress)),
    );
  }

  async closeAccountSessions(
    serverId: string,
    accountId: string,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.closeAccountSessions(serverId, accountId, adminPassword)),
    );
  }

  async deleteAccount(
    serverId: string,
    accountId: string,
    confirmation: string | null,
    adminPassword: string | null,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.deleteAccount(serverId, accountId, confirmation, adminPassword)),
    );
  }

  async getReauthState(serverId: string): Promise<ReauthState> {
    return unwrap(await commands.getReauthState(serverId));
  }

  async reauthCovers(kind: AdminActKind, role: Role | null): Promise<boolean> {
    return commands.reauthCovers(kind, role);
  }

  async setReauthSetting(
    serverId: string,
    mode: ReauthMode,
    password: string,
  ): Promise<ReauthSettingOutcome> {
    return toReauthSettingOutcome(
      unwrap(await commands.setReauthSetting(serverId, mode, password)),
    );
  }

  async listTrustedDevices(serverId: string): Promise<TrustedDevices> {
    return toTrustedDevices(unwrap(await commands.listTrustedDevices(serverId)));
  }

  async removeTrustedDevice(
    serverId: string,
    deviceId: string,
    password: string,
  ): Promise<DeviceRemovalOutcome> {
    return toDeviceRemovalOutcome(
      unwrap(await commands.removeTrustedDevice(serverId, deviceId, password)),
    );
  }

  async onSecurity(listener: (state: SecurityState) => void): Promise<Unsubscribe> {
    // Écoute posée D'ABORD, dernier état connu lu ensuite : aucune fenêtre où un état est perdu.
    const unlisten = await listen<SecurityEvent>(LINK_EVENTS.security, (event) =>
      listener(toSecurityState(event.payload)),
    );
    try {
      for (const state of await commands.listSecurityStates()) listener(toSecurityState(state));
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async onServices(listener: (event: ServicesEvent) => void): Promise<Unsubscribe> {
    // Écoute posée D'ABORD, dernier état connu lu ensuite : aucune fenêtre où un état est perdu. L'état
    // rejoué et le signal peuvent se croiser : le récepteur écarte tout `seq` qui n'est pas plus grand.
    const unlisten = await listen<BoundServicesEvent>(LINK_EVENTS.services, (event) =>
      listener(toServicesEvent(event.payload)),
    );
    try {
      for (const state of unwrap(await commands.listServicesStates())) {
        listener(toServicesEvent(state));
      }
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async onServiceOperations(
    listener: (event: ServiceOperationEvent) => void,
  ): Promise<Unsubscribe> {
    // Écoute posée D'ABORD, état tenu lu ensuite : aucune fenêtre où un changement est perdu. Le rejeu et le
    // signal peuvent se croiser : le récepteur écarte tout `seq` qui n'est pas plus grand.
    const unlisten = await listen<BoundServiceOperationEvent>(
      LINK_EVENTS.serviceOperations,
      (event) => listener(toServiceOperationEvent(event.payload)),
    );
    try {
      for (const change of unwrap(await commands.listServiceOperations())) {
        listener(toServiceOperationEvent(change));
      }
    } catch (error) {
      unlisten();
      throw error;
    }
    return unlisten;
  }

  async ackServiceOperations(serverId: string, opIds: string[]): Promise<void> {
    unwrap(await commands.ackServiceOperations(serverId, opIds));
  }

  async decideServiceOperation(
    serverId: string,
    opId: string,
    choice: OperationChoice,
    adminPassword: string | null,
  ): Promise<DecisionOutcome> {
    return toDecisionOutcome(
      unwrap(await commands.decideServiceOperation(serverId, opId, choice, adminPassword)),
    );
  }

  async pilotService(
    serverId: string,
    key: string,
    gesture: LifecycleGesture,
    adminPassword: string | null,
  ): Promise<LifecycleOutcome> {
    const run = {
      start: commands.serviceStart,
      stop: commands.serviceStop,
      restart: commands.serviceRestart,
    }[gesture];
    return toLifecycleOutcome(unwrap(await run(serverId, key, adminPassword)));
  }

  async recheckEngine(serverId: string): Promise<EngineInfo> {
    return toEngine(unwrap(await commands.engineRecheck(serverId)));
  }

  async readServiceRights(serverId: string): Promise<ServiceRights> {
    return toServiceRights(unwrap(await commands.readServiceRights(serverId)));
  }

  async revealServiceEnv(serverId: string, key: string): Promise<EnvVariable[]> {
    return toEnvVariables(unwrap(await commands.revealServiceEnv(serverId, key)));
  }

  async openServiceLogs(
    serverId: string,
    key: string,
    listener: (message: LogMessage) => void,
  ): Promise<LogReading> {
    // L'écoute est posée AVANT l'ouverture : le premier `status` peut arriver avant la réponse de la commande ; ce
    // qui arrive d'ici là est gardé, puis rendu dans l'ordre, pour cette session seulement.
    let session: string | null = null;
    let closed = false;
    let unlisten: () => void = () => undefined;
    const early: ServiceLogEvent[] = [];
    const finish = () => {
      closed = true;
      unlisten();
      if (session !== null) this.readings.delete(session);
      this.stopKeepAlive();
    };
    const deliver = (event: ServiceLogEvent) => {
      const message = toLogMessage(event.body);
      if (!message) return;
      listener(message);
      // Une fin (de l'agent ou de la coquille) libère la session côté coquille : plus rien à battre.
      if (message.kind === "ended") finish();
    };
    unlisten = await listen<ServiceLogEvent>(LINK_EVENTS.serviceLogs, (event) => {
      if (closed) return;
      if (session === null) early.push(event.payload);
      else if (event.payload.session === session) deliver(event.payload);
    });
    let mine: string;
    try {
      mine = await this.openLogSession(serverId, key);
    } catch (error) {
      finish();
      throw error;
    }
    session = mine;
    // Si la coquille ne tient plus la session (événement perdu), l'écran le sait comme pour une fermeture annoncée.
    this.readings.set(mine, () => {
      if (closed) return;
      listener({ kind: "ended", end: { kind: "closed", cause: "lost" } });
      finish();
    });
    this.keepTimer ??= setInterval(() => void this.beat(), this.keepAliveMs);
    for (const event of early.splice(0)) {
      if (event.session === mine && !closed) deliver(event);
    }
    return {
      close: () => {
        if (closed) return;
        finish();
        void commands.closeServiceLogs(mine).catch(() => undefined);
      },
      ack: (batches) => {
        if (!closed) void commands.ackServiceLogs(mine, batches).catch(() => undefined);
      },
    };
  }

  /** Ouvre la session côté coquille. Une génération de page inconnue : la page s'annonce de nouveau, une fois. */
  private async openLogSession(serverId: string, key: string, retried = false): Promise<string> {
    const page = await this.announceLogPage();
    try {
      return unwrap(await commands.openServiceLogs(page, serverId, key));
    } catch (error) {
      if (
        !retried &&
        error instanceof LinkCommandError &&
        error.failure.kind === "log_page_unknown"
      ) {
        this.logPage = null;
        return this.openLogSession(serverId, key, true);
      }
      throw error;
    }
  }

  async openEngineDocs(): Promise<void> {
    unwrap(await commands.openEngineDocs());
  }

  async getSecurity(serverId: string): Promise<SecurityRead> {
    return toSecurityRead(unwrap(await commands.getSecurity(serverId)));
  }

  async setAttackMode(
    serverId: string,
    active: boolean,
    password: string,
  ): Promise<AttackModeOutcome> {
    return toAttackModeOutcome(unwrap(await commands.setAttackMode(serverId, active, password)));
  }

  async getAgentUpdate(serverId: string): Promise<AgentUpdateView> {
    return unwrap(await commands.getAgentUpdate(serverId));
  }

  async ackAgentResult(serverId: string, at: string): Promise<void> {
    unwrap(await commands.ackAgentResult(serverId, at));
  }

  async updateAgent(
    serverId: string,
    version: string,
    adminPassword: string,
  ): Promise<AgentUpdateOutcome> {
    return toAgentUpdateOutcome(
      unwrap(await commands.updateAgent(serverId, version, adminPassword)),
    );
  }

  async onAgentUpdate(listener: (event: AgentUpdateEvent) => void): Promise<Unsubscribe> {
    // Un signal par message du flux : rien n'est rejoué, l'état qui compte se relit par la lecture.
    return listen<BoundAgentUpdateEvent>(LINK_EVENTS.agentUpdate, (event) =>
      listener(event.payload),
    );
  }

  async readAudit(
    serverId: string,
    filter: AuditFilter,
    before: number | null,
  ): Promise<AuditPage> {
    return toAuditPage(
      unwrap(await commands.readAudit(serverId, toAuditFilterDto(filter), before)),
    );
  }

  async exportAudit(serverId: string, filter: AuditFilter): Promise<AuditExportResult> {
    return unwrap(await commands.exportAudit(serverId, toAuditFilterDto(filter)));
  }

  async onAudit(
    serverId: string,
    listener: (entry: AuditEntry) => void,
    onGap: () => void = () => {},
  ): Promise<Unsubscribe> {
    const off: Unsubscribe[] = [];
    try {
      off.push(
        await listen<AuditLiveEvent>(LINK_EVENTS.audit, (event) => {
          const live = toAuditLive(event.payload);
          if (live && live.serverId === serverId) listener(live.entry);
        }),
      );
      off.push(await listen(LINK_EVENTS.auditGap, () => onGap()));
    } catch (error) {
      for (const stop of off) stop();
      throw error;
    }
    return () => {
      for (const stop of off) stop();
    };
  }
}
