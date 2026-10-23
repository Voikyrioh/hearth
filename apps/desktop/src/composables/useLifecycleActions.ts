import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { getLinkBridge, type LifecycleGesture } from "@/link";

/**
 * Démarrer, arrêter, redémarrer un service (HRT-65, BR-LIFE-016) : un acte d'administration confirmé, par UNE
 * commande typée du pont. Une seule à la fois (`useServerAction`), résultat inconnu à la coupure et JAMAIS rejoué
 * (BR-LIFE-020) : l'issue de l'action arrive par le suivi des opérations, jamais par cette réponse. Aucun mot de passe
 * n'est gardé ici : il traverse l'appel et c'est tout.
 */
export function useLifecycleActions(serverId: () => string) {
  const action = useServerAction(serverId);

  async function perform(
    key: string,
    gesture: LifecycleGesture,
    adminPassword: string | null,
  ): Promise<ActReport> {
    const result = await action.run(
      () => getLinkBridge().pilotService(serverId(), key, gesture, adminPassword),
      {
        unknownMessage: "lifecycle.unknownResult",
        forbiddenMessage: "lifecycle.forbidden",
        passwordMissingIsRefusal: true,
      },
    );
    // L'état est passé à « attaque » entre la lecture et l'envoi : même parcours que `password_required`.
    if (!result && action.passwordMissing.value) {
      return { kind: "refused", refusal: { kind: "password_required" } };
    }
    if (!result) return action.failedReport();
    if (result.kind === "accepted") return { kind: "done" };
    if (result.kind === "refused") return { kind: "refused", refusal: result.refusal };
    return { kind: "unknown" };
  }

  return { perform, busy: action.busy };
}
