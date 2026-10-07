import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import { type AttackModeRefusal, getLinkBridge } from "@/link";
import { useSecurityStore } from "@/stores/security";
import { useToastsStore } from "@/stores/toasts";

/**
 * Ce que l'appelant (la fenêtre de confirmation) doit savoir d'un changement : `done` (annoncé, état
 * relu), `refused` (à montrer dans la fenêtre), `unknown` (le lien est tombé avant la réponse : déjà dit,
 * JAMAIS rejoué, l'état se relit au retour du lien), `failed` (rien n'est parti ou rôle refusé : déjà
 * notifié).
 */
export type AttackModeReport =
  | { kind: "done" }
  | { kind: "refused"; refusal: AttackModeRefusal }
  | { kind: "unknown" }
  | { kind: "failed" };

/**
 * L'activation et la désactivation du mode attaque d'un serveur. Elles passent par `useServerAction`
 * (une seule à la fois, résultat inconnu à la coupure, jamais rejouée) et par UNE commande typée du
 * pont. Le mot de passe n'est gardé nulle part : il traverse l'appel et c'est tout (l'appelant vide son
 * champ après l'envoi, réussi ou non).
 */
export function useAttackModeActions(serverId: () => string) {
  const action = useServerAction();
  const toasts = useToastsStore();
  const security = useSecurityStore();

  return {
    busy: action.busy,

    async change(active: boolean, password: string): Promise<AttackModeReport> {
      const result = await action.run(
        () => getLinkBridge().setAttackMode(serverId(), active, password),
        {
          forbiddenMessage: "security.noPermission",
        },
      );
      if (!result) return { kind: "failed" };
      if (result.kind === "done") {
        toasts.push({
          kind: "success",
          message: t(active ? "security.enabled" : "security.disabled"),
        });
        void security.load(serverId());
        return { kind: "done" };
      }
      if (result.kind === "refused") return { kind: "refused", refusal: result.refusal };
      return { kind: "unknown" };
    },
  };
}
