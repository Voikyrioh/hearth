import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { getLinkBridge, type ServerInfo, type Unsubscribe } from "@/link";

/**
 * Serveurs enregistrés, alimentés par le pont de liaison. `currentId` est le serveur
 * affiché (posé par le gabarit de serveur d'après la route) : la directive `v-needs-link`
 * et les composants de la coquille y lisent « le serveur courant ».
 */
export const useServersStore = defineStore("servers", () => {
  const servers = ref<ServerInfo[]>([]);
  const loaded = ref(false);
  const currentId = ref<string | null>(null);
  let loading: Promise<void> | null = null;
  let unsubscribe: Unsubscribe | null = null;

  const current = computed(() => servers.value.find((s) => s.id === currentId.value) ?? null);
  const first = computed(() => servers.value[0] ?? null);

  /** Charge la liste une seule fois et suit ses changements. */
  function load(): Promise<void> {
    loading ??= (async () => {
      const bridge = getLinkBridge();
      servers.value = await bridge.listServers();
      unsubscribe = bridge.onServersChanged((next) => {
        servers.value = next;
      });
      loaded.value = true;
    })().catch((error) => {
      loading = null;
      throw error;
    });
    return loading;
  }

  function byId(id: string): ServerInfo | undefined {
    return servers.value.find((s) => s.id === id);
  }

  function setCurrent(id: string | null) {
    currentId.value = id;
  }

  /** Remise à zéro (tests). */
  function reset() {
    unsubscribe?.();
    unsubscribe = null;
    loading = null;
    servers.value = [];
    loaded.value = false;
    currentId.value = null;
  }

  return { servers, loaded, currentId, current, first, load, byId, setCurrent, reset };
});
