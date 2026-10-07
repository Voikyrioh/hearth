import type { AgentUpdateEvent } from "./agent-update";
import type { AuditEntry } from "./audit";
import type { LinkBridge } from "./bridge";
import type { MachineEvent } from "./machine";
import type { SecurityState } from "./security";
import {
  type FingerprintChange,
  LinkCommandError,
  type LinkNotice,
  type LinkStateEvent,
  type OperationEvent,
  type ServerInfo,
  type Unsubscribe,
} from "./types";

/**
 * Pont vide : aucun serveur, aucun événement, toute commande échoue (« incident interne »).
 * Sert hors de la fenêtre Tauri quand le pont simulé n'est pas de la partie (build livré servi
 * dans un navigateur) : jamais de faux succès.
 */
export class NullLinkBridge implements LinkBridge {
  async onServersChanged(listener: (servers: ServerInfo[]) => void): Promise<Unsubscribe> {
    listener([]);
    return () => {};
  }
  async onLinkState(_listener: (event: LinkStateEvent) => void): Promise<Unsubscribe> {
    return () => {};
  }
  async retryNow(): Promise<void> {}
  async onOperation(_listener: (event: OperationEvent) => void): Promise<Unsubscribe> {
    return () => {};
  }
  async onFingerprintChanged(_listener: (change: FingerprintChange) => void): Promise<Unsubscribe> {
    return () => {};
  }
  async onNotice(_listener: (notice: LinkNotice) => void): Promise<Unsubscribe> {
    return () => {};
  }
  async onMachine(
    _serverId: string,
    _listener: (event: MachineEvent) => void,
  ): Promise<Unsubscribe> {
    return () => {};
  }
  private unavailable(): never {
    throw new LinkCommandError({ kind: "internal" });
  }
  async probeServer(): Promise<never> {
    return this.unavailable();
  }
  async addAndLogin(): Promise<never> {
    return this.unavailable();
  }
  async login(): Promise<never> {
    return this.unavailable();
  }
  async logout(): Promise<never> {
    return this.unavailable();
  }
  async acceptFingerprint(): Promise<never> {
    return this.unavailable();
  }
  async updateServer(): Promise<never> {
    return this.unavailable();
  }
  async removeServer(): Promise<never> {
    return this.unavailable();
  }
  async forgetCredentials(): Promise<never> {
    return this.unavailable();
  }
  async setDisplayedServer(): Promise<void> {}
  async checkAccountInput(): Promise<never> {
    return this.unavailable();
  }
  async listAccounts(): Promise<never> {
    return this.unavailable();
  }
  async createAccount(): Promise<never> {
    return this.unavailable();
  }
  async changeAccountRole(): Promise<never> {
    return this.unavailable();
  }
  async setAccountPassword(): Promise<never> {
    return this.unavailable();
  }
  async changeOwnPassword(): Promise<never> {
    return this.unavailable();
  }
  async closeAccountSessions(): Promise<never> {
    return this.unavailable();
  }
  async deleteAccount(): Promise<never> {
    return this.unavailable();
  }
  async getReauthState(): Promise<never> {
    return this.unavailable();
  }
  async reauthCovers(): Promise<never> {
    return this.unavailable();
  }
  async setReauthSetting(): Promise<never> {
    return this.unavailable();
  }
  async listTrustedDevices(): Promise<never> {
    return this.unavailable();
  }
  async removeTrustedDevice(): Promise<never> {
    return this.unavailable();
  }
  async onSecurity(_listener: (state: SecurityState) => void): Promise<Unsubscribe> {
    return () => {};
  }

  async getSecurity(): Promise<never> {
    return this.unavailable();
  }

  async setAttackMode(): Promise<never> {
    return this.unavailable();
  }

  async getAgentUpdate(): Promise<never> {
    return this.unavailable();
  }
  async ackAgentResult(): Promise<void> {}
  async updateAgent(): Promise<never> {
    return this.unavailable();
  }
  async onAgentUpdate(_listener: (event: AgentUpdateEvent) => void): Promise<Unsubscribe> {
    return () => {};
  }
  async readAudit(): Promise<never> {
    return this.unavailable();
  }
  async exportAudit(): Promise<never> {
    return this.unavailable();
  }
  async onAudit(
    _serverId: string,
    _listener: (entry: AuditEntry) => void,
    _onGap?: () => void,
  ): Promise<Unsubscribe> {
    return () => {};
  }
}
