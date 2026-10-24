import { defineStore } from "pinia";
import { reactive } from "vue";
import type { SettingEffect } from "@/link";
import { useServersStore } from "./servers";

/**
 * Les réglages enregistrés qui ne sont pas encore appliqués (HRT-109, BR-FICHE-042) : « À appliquer au prochain démarrage »
 * ou « À appliquer quand Hearth recrée le service ». L'agent ne dit pas ce qui attend ; le client s'en souvient, par
 * service, EN MÉMOIRE seulement (jamais une valeur, jamais un secret : seulement l'effet et l'heure). Le repère disparaît
 * quand le service a redémarré depuis (`since` postérieur à l'enregistrement) ; un serveur retiré du carnet est oublié.
 */
export interface PendingApply {
  effect: Exclude<SettingEffect, "none">;
  /** Quand les réglages ont été enregistrés (ms). */
  at: number;
}

export const useGameSettingsPendingStore = defineStore("gameSettingsPending", () => {
  const items = reactive(new Map<string, PendingApply>());
  const keyOf = (serverId: string, serviceKey: string) => `${serverId}/${serviceKey}`;

  function of(serverId: string, serviceKey: string): PendingApply | undefined {
    return items.get(keyOf(serverId, serviceKey));
  }

  /** Des réglages viennent d'être enregistrés : s'il y a quelque chose à appliquer, on s'en souvient (le plus lourd d'abord). */
  function note(serverId: string, serviceKey: string, effect: SettingEffect, at: number): void {
    if (effect === "none") return;
    const known = items.get(keyOf(serverId, serviceKey));
    const heavier = known?.effect === "recreate" ? "recreate" : effect;
    items.set(keyOf(serverId, serviceKey), { effect: heavier, at });
  }

  function clear(serverId: string, serviceKey: string): void {
    items.delete(keyOf(serverId, serviceKey));
  }

  const servers = useServersStore();
  servers.$subscribe(() => {
    if (!servers.loaded) return;
    const ids = servers.servers.map((server) => server.id);
    for (const key of [...items.keys()]) {
      if (!ids.includes(key.slice(0, key.indexOf("/")))) items.delete(key);
    }
  });

  return { of, note, clear };
});
