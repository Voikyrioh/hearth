import { defineStore } from "pinia";
import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { failureOf, getLinkBridge, type TrustedDevice } from "@/link";

/**
 * Les postes de confiance de chaque serveur, tels que l'agent les a rendus : des noms, des dates, une
 * adresse et des booléens, JAMAIS une clé (la WebView n'en reçoit aucune). Rien n'est stocké hors de la
 * mémoire de la page. Une liste déjà lue reste affichée pendant qu'on la relit (aucun clignotement) et,
 * si la relecture échoue, elle n'est ni vidée ni fausse : juste `error`. Une lecture plus ancienne
 * qu'une autre ne l'écrase jamais.
 */
export type DevicesStatus = "loading" | "ready" | "unsupported" | "error";

export interface ServerDevices {
  status: DevicesStatus;
  devices: TrustedDevice[];
  /** Postes au plus par compte ; 8 tant que la liste n'a pas été lue. */
  max: number;
}

export const useDevicesStore = defineStore("devices", () => {
  const byServer = ref<Record<string, ServerDevices>>({});
  const reads = new Map<string, number>();

  function of(serverId: string): ServerDevices | undefined {
    return byServer.value[serverId];
  }

  function put(serverId: string, next: ServerDevices) {
    byServer.value = { ...byServer.value, [serverId]: next };
  }

  /** Lit (ou relit) la liste ; une réponse plus ancienne que la dernière demandée est écartée. */
  async function load(serverId: string): Promise<void> {
    const ticket = (reads.get(serverId) ?? 0) + 1;
    reads.set(serverId, ticket);
    const known = byServer.value[serverId];
    if (!known || known.status === "error" || known.status === "unsupported") {
      put(serverId, { status: "loading", devices: known?.devices ?? [], max: known?.max ?? 8 });
    }
    try {
      const list = await getLinkBridge().listTrustedDevices(serverId);
      if (reads.get(serverId) !== ticket) return;
      if (list.kind === "unsupported")
        put(serverId, { status: "unsupported", devices: [], max: 8 });
      else put(serverId, { status: "ready", devices: list.devices, max: list.max });
    } catch (error) {
      if (reads.get(serverId) !== ticket) return;
      // Lien coupé ou serveur parti : la dernière liste connue reste (le gabarit la désature).
      const current = byServer.value[serverId];
      put(serverId, {
        status: "error",
        devices: current?.devices ?? [],
        max: current?.max ?? 8,
      });
      if (!failureOf(error)) reportUiError(error, "devices:load");
    }
  }

  function forget(serverId: string) {
    const { [serverId]: _gone, ...rest } = byServer.value;
    byServer.value = rest;
    reads.delete(serverId);
  }

  function reset() {
    byServer.value = {};
    reads.clear();
  }

  return { byServer, of, load, forget, reset };
});
