import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type ActionInput,
  type ActionResult,
  failureMessage,
  failureOf,
  getLinkBridge,
} from "@/link";
import { useToastsStore } from "@/stores/toasts";

/**
 * Lance une action sur un serveur (le bouton qui l'appelle porte `needs-link`, BR-RESIL-008).
 * Le lien tombe avant la réponse : « Le résultat de cette action n'est pas connu. » puis la
 * suggestion, jamais de rejeu (BR-RESIL-009) ; l'issue arrive plus tard par le store du lien
 * (« Fait pendant la coupure », « Non exécuté. Tu peux relancer. », « Résultat inconnu. »,
 * BR-RESIL-010). Aucune fenêtre bloquante : tout passe par des notifications discrètes (BR-RESIL-011).
 */
export function useServerAction() {
  const toasts = useToastsStore();
  const busy = ref(false);
  /** Dernière action restée sans réponse : sa clé d'opération (`link.outcomeOf` dit l'issue). */
  const unknownOpId = ref<string | null>(null);

  async function run(serverId: string, action: ActionInput): Promise<ActionResult | null> {
    if (busy.value) return null;
    busy.value = true;
    try {
      const result = await getLinkBridge().runAction(serverId, action);
      if (result.kind === "unknown") {
        unknownOpId.value = result.opId;
        toasts.push({ kind: "warn", message: t("operation.unknownNow") });
        toasts.push({ kind: "info", message: t("operation.unknownHint") });
      } else {
        unknownOpId.value = null;
      }
      return result;
    } catch (error) {
      const failure = failureOf(error);
      if (failure) toasts.push({ kind: "error", message: failureMessage(failure) });
      else reportUiError(error, "action");
      return null;
    } finally {
      busy.value = false;
    }
  }

  return { run, busy, unknownOpId };
}
