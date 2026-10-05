import { listen } from "@tauri-apps/api/event";
import {
  type LinkFailure as BoundFailure,
  commands,
  type FingerprintEvent,
  type LinkStateDto,
  type NoticeEvent,
  type OperationEventDto,
  type ServerDto,
  type ServersEvent,
} from "@/bindings";
import type { LinkBridge } from "./bridge";
import {
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

  async retryNow(serverId: string): Promise<void> {
    unwrap(await commands.retryNow(serverId));
  }

  async onOperation(listener: (event: OperationEvent) => void): Promise<Unsubscribe> {
    return listen<OperationEventDto>(LINK_EVENTS.operation, (event) => listener(event.payload));
  }

  async onFingerprintChanged(listener: (change: FingerprintChange) => void): Promise<Unsubscribe> {
    return listen<FingerprintEvent>(LINK_EVENTS.fingerprint, (event) => listener(event.payload));
  }

  async onNotice(listener: (notice: LinkNotice) => void): Promise<Unsubscribe> {
    return listen<NoticeEvent>(LINK_EVENTS.notice, (event) => listener(event.payload));
  }

  async probeServer(host: string, port: number | null): Promise<ProbeResult> {
    return unwrap(await commands.probeServer(host, port));
  }

  async addServer(input: NewServerInput): Promise<ServerInfo> {
    const dto = unwrap(
      await commands.addServer(
        input.name,
        input.color,
        input.host,
        input.port,
        input.fingerprint,
        input.macAddresses,
      ),
    );
    return toServerInfo(dto);
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
}
