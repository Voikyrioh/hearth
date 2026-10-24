import { defineStore } from "pinia";
import { reactive } from "vue";
import { failureOf, getLinkBridge } from "@/link";
import { useServersStore } from "./servers";

/**
 * Quels services ont des réglages de jeu (HRT-109, BR-FICHE-054). La liste des services ne dit pas qu'un service est né d'une
 * fiche : le client le demande UNE fois par service à l'agent (`GET /services/{key}/settings`, une lecture qui répond
 * « ce service n'a pas de réglages » pour un service ordinaire ou adopté) et garde la réponse en MÉMOIRE (jamais une valeur
 * de réglage : seulement oui ou non). Un échec ne se garde pas : on redemandera. Un serveur retiré du carnet est oublié.
 */
export const useGameSettingsPresenceStore = defineStore("gameSettingsPresence", () => {
  const known = reactive(new Map<string, boolean>());
  const asked = new Set<string>();
  const keyOf = (serverId: string, serviceKey: string) => `${serverId}/${serviceKey}`;

  /** `true` / `false` quand l'agent a répondu, `null` tant qu'on ne sait pas. */
  function has(serverId: string, serviceKey: string): boolean | null {
    return known.get(keyOf(serverId, serviceKey)) ?? null;
  }

  async function check(serverId: string, serviceKey: string): Promise<void> {
    const key = keyOf(serverId, serviceKey);
    if (known.has(key) || asked.has(key)) return;
    asked.add(key);
    try {
      const read = await getLinkBridge().readGameSettings(serverId, serviceKey);
      known.set(key, read.kind === "found");
    } catch (error) {
      // Pas le droit, lien coupé, agent muet : on ne sait pas, et on redemandera.
      void failureOf(error);
    } finally {
      asked.delete(key);
    }
  }

  function forget(serverId: string, serviceKey: string): void {
    known.delete(keyOf(serverId, serviceKey));
  }

  const servers = useServersStore();
  servers.$subscribe(() => {
    if (!servers.loaded) return;
    const ids = servers.servers.map((server) => server.id);
    for (const key of [...known.keys()]) {
      if (!ids.includes(key.slice(0, key.indexOf("/")))) known.delete(key);
    }
  });

  return { has, check, forget };
});
