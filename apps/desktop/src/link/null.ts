import type { LinkBridge } from "./bridge";
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
  private unavailable(): never {
    throw new LinkCommandError({ kind: "internal" });
  }
  async probeServer(): Promise<never> {
    return this.unavailable();
  }
  async addServer(): Promise<never> {
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
}
