import { computed, watch } from "vue";
import { useAgentUpdatesStore } from "@/stores/agentUpdates";

/**
 * La version de l'agent d'un serveur (« Livrée avec Hearth {version} », BR-FICHE-003), lue chez l'agent s'il ne l'a pas
 * encore dite. `null` tant qu'elle n'est pas connue : la phrase se dit alors sans numéro, jamais avec un numéro inventé.
 */
export function useAgentVersion(serverId: () => string) {
  const agents = useAgentUpdatesStore();
  watch(
    serverId,
    (id) => {
      if (id && !agents.entry(id).view) void agents.refresh(id);
    },
    { immediate: true },
  );
  return computed(() => {
    const id = serverId();
    return id ? (agents.entry(id).view?.current ?? null) : null;
  });
}
