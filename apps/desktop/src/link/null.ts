import type { LinkBridge } from "./bridge";
import type { LinkStateEvent, OperationEvent, ServerInfo, Unsubscribe } from "./types";

/** Pont vide : aucun serveur, aucun événement. Sert tant que le pont réel n'est pas branché. */
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
}
