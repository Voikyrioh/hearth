import type { UpdateStateDto } from "@/bindings";

export type Unsubscribe = () => void;

/**
 * Pont de la mise à jour du client : la seule porte de l'interface vers la coquille pour ce sujet.
 * Aucune méthode ne prend d'adresse, de chemin ni de clé : l'interface demande, la coquille
 * décide et fait (ADR-0017). Les trois actions rendent l'état qui en résulte.
 */
export interface UpdateBridge {
  /** L'état courant. */
  getState(): Promise<UpdateStateDto>;
  /** « Vérifier maintenant ». */
  check(): Promise<UpdateStateDto>;
  /** « Plus tard » : le bandeau disparaît 24 h. */
  postpone(): Promise<UpdateStateDto>;
  /** « Mettre à jour maintenant » : rend la main tout de suite, l'issue arrive par `onState`. */
  install(): Promise<UpdateStateDto>;
  /** Chaque état publié par la coquille (le dernier est rejoué par `getState`, pas ici). */
  onState(listener: (state: UpdateStateDto) => void): Promise<Unsubscribe>;
}
