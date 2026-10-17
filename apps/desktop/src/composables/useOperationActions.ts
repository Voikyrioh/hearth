import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { getLinkBridge, type OperationChoice } from "@/link";

/**
 * Répondre à une opération de service (HRT-84, BR-SVC-029) : un acte d'administration confirmé, par UNE commande
 * typée du pont. Une seule à la fois (`useServerAction`), résultat inconnu à la coupure et jamais rejouée : l'état
 * de l'opération arrive par le flux. Aucun mot de passe n'est gardé ici : il traverse l'appel et c'est tout.
 */
export function useOperationActions(serverId: () => string) {
  const action = useServerAction(serverId);

  async function decide(
    opId: string,
    choice: OperationChoice,
    adminPassword: string | null,
  ): Promise<ActReport> {
    const result = await action.run(
      () => getLinkBridge().decideServiceOperation(serverId(), opId, choice, adminPassword),
      { unknownMessage: "serviceOps.unknownResult", forbiddenMessage: "serviceOps.forbidden" },
    );
    if (!result) return action.failedReport();
    if (result.kind === "done") return { kind: "done" };
    if (result.kind === "refused") return { kind: "refused", refusal: result.refusal };
    return { kind: "unknown" };
  }

  return { decide, busy: action.busy };
}
