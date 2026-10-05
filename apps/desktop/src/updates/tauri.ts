import { listen } from "@tauri-apps/api/event";
import { commands, type UpdateStateDto } from "@/bindings";
import type { Unsubscribe, UpdateBridge } from "./bridge";

/** Nom de l'événement de la coquille (`update/dto.rs::STATE_EVENT`). */
export const UPDATE_STATE_EVENT = "update://state";

/** Pont réel : les commandes typées et l'événement `update://state` de la coquille. */
export class TauriUpdateBridge implements UpdateBridge {
  getState(): Promise<UpdateStateDto> {
    return commands.getUpdateState();
  }
  check(): Promise<UpdateStateDto> {
    return commands.checkForUpdates();
  }
  postpone(): Promise<UpdateStateDto> {
    return commands.postponeUpdate();
  }
  install(): Promise<UpdateStateDto> {
    return commands.installUpdate();
  }
  async onState(listener: (state: UpdateStateDto) => void): Promise<Unsubscribe> {
    return listen<UpdateStateDto>(UPDATE_STATE_EVENT, (event) => listener(event.payload));
  }
}
