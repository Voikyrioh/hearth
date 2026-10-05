import { computed } from "vue";
import type { LinkState } from "@/link";
import { useLinkStore } from "@/stores/link";
import { useServersStore } from "@/stores/servers";

/** Le serveur affiché, son rôle et l'état de son lien. */
export function useCurrentServer() {
  const servers = useServersStore();
  const link = useLinkStore();

  const server = computed(() => servers.current);
  const state = computed<LinkState | null>(() =>
    server.value ? link.stateOf(server.value.id) : null,
  );
  const lastContactAt = computed(() =>
    server.value ? (link.eventOf(server.value.id)?.lastContactAt ?? null) : null,
  );
  const isConnected = computed(() => state.value === "connected");
  const isAdmin = computed(() => server.value?.role === "admin");

  return { server, state, lastContactAt, isConnected, isAdmin };
}
