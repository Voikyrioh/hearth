import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import { type ActionResult, failureMessage, failureOf } from "@/link";
import { useToastsStore } from "@/stores/toasts";

/**
 * Lance une action sur un serveur : `perform` appelle UNE commande typée du pont (la méthode et le
 * chemin sont construits côté Rust ; le bouton qui l'appelle porte `needs-link`, BR-RESIL-008). Le
 * lien tombe avant la réponse : « Le résultat de cette action n'est pas connu. » puis la suggestion,
 * jamais de rejeu (BR-RESIL-009) ; l'issue arrive plus tard par le store du lien (« Fait pendant la
 * coupure », « Non exécuté. Tu peux relancer. », « Résultat inconnu. », BR-RESIL-010). Aucune fenêtre
 * bloquante : tout passe par des notifications discrètes (BR-RESIL-011).
 */
export function useServerAction() {
  const toasts = useToastsStore();
  const busy = ref(false);

  async function run(perform: () => Promise<ActionResult>): Promise<ActionResult | null> {
    if (busy.value) return null;
    busy.value = true;
    try {
      const result = await perform();
      if (result.kind === "unknown") {
        toasts.push({ kind: "warn", message: t("operation.unknownNow") });
        toasts.push({ kind: "info", message: t("operation.unknownHint") });
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

  return { run, busy };
}
