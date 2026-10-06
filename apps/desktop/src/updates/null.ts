import type { UpdateStateDto } from "@/bindings";
import type { Unsubscribe, UpdateBridge } from "./bridge";

/**
 * Pont vide : toute commande échoue, aucun événement. Sert hors de la fenêtre Tauri quand le pont
 * simulé n'est pas de la partie (build livré servi dans un navigateur) : jamais de faux succès.
 */
export class NullUpdateBridge implements UpdateBridge {
  private fail<T>(): Promise<T> {
    return Promise.reject(new Error("pont de mise à jour indisponible"));
  }
  getState(): Promise<UpdateStateDto> {
    return this.fail();
  }
  check(): Promise<UpdateStateDto> {
    return this.fail();
  }
  postpone(): Promise<UpdateStateDto> {
    return this.fail();
  }
  install(): Promise<UpdateStateDto> {
    return this.fail();
  }
  async onState(_listener: (state: UpdateStateDto) => void): Promise<Unsubscribe> {
    return () => {};
  }
}
