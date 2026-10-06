import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { type MessageKey, t } from "@/i18n";
import { failureMessage, failureOf } from "@/link";
import { useToastsStore } from "@/stores/toasts";

/** Ce que `perform` rend : tout résultat typé d'une commande, dont le « résultat inconnu ». */
export interface ActionOutcome {
  kind: string;
}

export interface RunOptions {
  /**
   * Le texte unique à montrer quand le lien tombe avant la réponse (la spécification de l'écran le
   * dit à sa façon). Par défaut : « Le résultat de cette action n'est pas connu. » puis la
   * suggestion de vérifier l'état.
   */
  unknownMessage?: MessageKey;
  /** Le texte du refus de rôle de l'agent (`forbidden`) propre à l'écran ; sinon celui de `failureMessage`. */
  forbiddenMessage?: MessageKey;
}

/**
 * Lance une action sur un serveur : `perform` appelle UNE commande typée du pont (la méthode et le
 * chemin sont construits côté Rust ; le bouton qui l'appelle porte `needs-link`, BR-RESIL-008). Le
 * lien tombe avant la réponse (`kind: "unknown"`) : on le dit, puis jamais de rejeu (BR-RESIL-009) ;
 * l'issue arrive plus tard par le store du lien (« Fait pendant la coupure », « Non exécuté. Tu peux
 * relancer. », « Résultat inconnu. », BR-RESIL-010). Aucune fenêtre bloquante : tout passe par des
 * notifications discrètes (BR-RESIL-011). Une action à la fois par appelant (`busy`). Rend `null`
 * si l'action n'a pas pu partir ou a échoué (déjà notifié).
 */
export function useServerAction() {
  const toasts = useToastsStore();
  const busy = ref(false);

  async function run<T extends ActionOutcome>(
    perform: () => Promise<T>,
    options: RunOptions = {},
  ): Promise<T | null> {
    if (busy.value) return null;
    busy.value = true;
    try {
      const result = await perform();
      if (result.kind === "unknown") {
        if (options.unknownMessage) {
          toasts.push({ kind: "warn", message: t(options.unknownMessage) });
        } else {
          toasts.push({ kind: "warn", message: t("operation.unknownNow") });
          toasts.push({ kind: "info", message: t("operation.unknownHint") });
        }
      }
      return result;
    } catch (error) {
      const failure = failureOf(error);
      if (failure) {
        const message =
          failure.kind === "forbidden" && options.forbiddenMessage
            ? t(options.forbiddenMessage)
            : failureMessage(failure);
        toasts.push({ kind: "error", message });
      } else reportUiError(error, "action");
      return null;
    } finally {
      busy.value = false;
    }
  }

  return { run, busy };
}
