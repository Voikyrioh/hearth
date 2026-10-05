import { createPinia, setActivePinia } from "pinia";
import { SimulatedLinkBridge, type SimulatedOptions, setLinkBridge } from "@/link";
import { useLinkStore } from "@/stores/link";
import { useServersStore } from "@/stores/servers";

/** Pinia neuve + pont simulé installé (aucune fuite d'un test à l'autre). */
export function freshBridge(options: SimulatedOptions = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const bridge = new SimulatedLinkBridge({ retryDelayMs: -1, ...options });
  setLinkBridge(bridge);
  return { pinia, bridge };
}

/** Comme `freshBridge`, avec les stores chargés et à l'écoute du pont. */
export async function startedApp(options: SimulatedOptions = {}) {
  const ctx = freshBridge(options);
  const servers = useServersStore();
  const link = useLinkStore();
  await servers.load();
  link.start();
  return { ...ctx, servers, link };
}
