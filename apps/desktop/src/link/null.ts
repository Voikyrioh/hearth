import type { LinkBridge } from "./bridge";
import type { ServerInfo } from "./types";

/** Pont vide : aucun serveur, aucun événement. Sert tant que le pont réel n'est pas branché. */
export class NullLinkBridge implements LinkBridge {
  async listServers(): Promise<ServerInfo[]> {
    return [];
  }
  onServersChanged() {
    return () => {};
  }
  onLinkState() {
    return () => {};
  }
  async retryNow() {}
  onOperation() {
    return () => {};
  }
}
