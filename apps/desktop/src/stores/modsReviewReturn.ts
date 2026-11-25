import { defineStore } from "pinia";
import { reactive } from "vue";

/**
 * Les services dont l'écran « Vérifiez avant d'appliquer » a ouvert la console : en revenant sur l'onglet Mods, le lot
 * reparaît sur cet écran (pas sur la liste), puisque c'est de là qu'on est parti ; et la console propose le lien de
 * retour. Tenu hors des composants, qui sont détruits au changement d'onglet. Oublié dès qu'on quitte le parcours mods
 * (autre onglet que Mods et Console, ou détail du service fermé).
 */
export const useModsReviewReturnStore = defineStore("modsReviewReturn", () => {
  const cameFromReview = reactive(new Set<string>());

  function keyOf(serverId: string, serviceKey: string): string {
    return `${serverId}/${serviceKey}`;
  }

  function mark(serverId: string, serviceKey: string): void {
    cameFromReview.add(keyOf(serverId, serviceKey));
  }

  function has(serverId: string, serviceKey: string): boolean {
    return cameFromReview.has(keyOf(serverId, serviceKey));
  }

  /** Rend `true` quand le service avait ouvert la console depuis l'écran de vérification (et l'oublie). */
  function take(serverId: string, serviceKey: string): boolean {
    return cameFromReview.delete(keyOf(serverId, serviceKey));
  }

  /** On a quitté le parcours mods : plus rien à reprendre. */
  function forget(serverId: string, serviceKey: string): void {
    cameFromReview.delete(keyOf(serverId, serviceKey));
  }

  return { mark, has, take, forget };
});
