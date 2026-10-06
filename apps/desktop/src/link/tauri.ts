import { listen } from "@tauri-apps/api/event";
import {
  type AccountListDto,
  type AuditLiveEvent,
  type LinkFailure as BoundFailure,
  type AccountOutcome as BoundOutcome,
  commands,
  type FingerprintEvent,
  type LinkStateDto,
  type MetricsEvent,
  type NoticeEvent,
  type OperationEventDto,
  type ServerDto,
  type ServersEvent,
  type SnapshotEvent,
} from "@/bindings";
import type { AuditEntry, AuditExportResult, AuditFilter, AuditPage } from "./audit";
import { toAuditFilterDto, toAuditLive, toAuditPage } from "./audit";
import type { LinkBridge } from "./bridge";
import { type MachineEvent, toMetrics, toView } from "./machine";
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
export const LINK_EVENTS = {
  servers: "link://servers",
  state: "link://state",
  operation: "link://operation",
  fingerprint: "link://fingerprint",
  notice: "link://notice",
  snapshot: "link://snapshot",
  metrics: "link://metrics",
  audit: "link://audit",
  auditGap: "link://audit-gap",
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

  constructor(now: () => number = Date.now) {
    this.now = now;
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
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.createAccount(serverId, username, password, role)),
    );
  }

  async changeAccountRole(
    serverId: string,
    accountId: string,
    role: Role,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(unwrap(await commands.changeAccountRole(serverId, accountId, role)));
  }

  async setAccountPassword(
    serverId: string,
    accountId: string,
    password: string,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.setAccountPassword(serverId, accountId, password)),
    );
  }

  async changeOwnPassword(
    serverId: string,
    current: string,
    password: string,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(unwrap(await commands.changeOwnPassword(serverId, current, password)));
  }

  async closeAccountSessions(serverId: string, accountId: string): Promise<AccountOutcome> {
    return toAccountOutcome(unwrap(await commands.closeAccountSessions(serverId, accountId)));
  }

  async deleteAccount(
    serverId: string,
    accountId: string,
    confirmation: string | null,
  ): Promise<AccountOutcome> {
    return toAccountOutcome(
      unwrap(await commands.deleteAccount(serverId, accountId, confirmation)),
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
