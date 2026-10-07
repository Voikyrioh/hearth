import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import { type DeviceRemovalRefusal, getLinkBridge, type TrustedDevice } from "@/link";
import { useDevicesStore } from "@/stores/devices";
import { useToastsStore } from "@/stores/toasts";

/**
 * Ce que l'appelant (la fenêtre de confirmation) doit savoir d'un retrait : `done` (annoncé, liste
 * relue), `refused` (à montrer dans la fenêtre), `unknown` (le lien est tombé avant la réponse : déjà dit,
 * JAMAIS rejoué, la liste se relit au retour du lien), `failed` (rien n'est parti : déjà notifié).
 */
export type RemovalReport =
  | { kind: "done" }
  | { kind: "refused"; refusal: DeviceRemovalRefusal }
  | { kind: "unknown" }
  | { kind: "failed" };

/**
 * Les actions de postes de confiance d'un serveur. Elles passent par `useServerAction` (une seule à la
 * fois, résultat inconnu à la coupure, jamais rejouée) et par UNE commande typée du pont. Le mot de
 * passe n'est gardé nulle part : il traverse l'appel et c'est tout (l'appelant vide son champ après
 * l'envoi, réussi ou non).
 */
export function useDeviceActions(serverId: () => string) {
  const action = useServerAction();
  const toasts = useToastsStore();
  const devices = useDevicesStore();

  return {
    busy: action.busy,

    async remove(device: TrustedDevice, password: string): Promise<RemovalReport> {
      const result = await action.run(() =>
        getLinkBridge().removeTrustedDevice(serverId(), device.id, password),
      );
      if (!result) return { kind: "failed" };
      if (result.kind === "done") {
        toasts.push({ kind: "success", message: t("devices.removed") });
        void devices.load(serverId());
        return { kind: "done" };
      }
      if (result.kind === "refused") {
        // La liste affichée est périmée (poste déjà retiré ailleurs) : on la relit.
        if (result.refusal.kind === "not_found") void devices.load(serverId());
        return { kind: "refused", refusal: result.refusal };
      }
      return { kind: "unknown" };
    },
  };
}
