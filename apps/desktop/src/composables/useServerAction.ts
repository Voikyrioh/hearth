import { ref } from "vue";
import { reportUiError } from "@/errors/report";
import { type MessageKey, t } from "@/i18n";
import { failureMessage, failureOf } from "@/link";
import { useServiceRightsStore } from "@/stores/serviceRights";
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
  /**
   * Le texte quand l'agent refuse l'acte faute du droit de gérer les services ET que ce refus est celui d'un
   * compte protégé (BR-GRANT-005), pas un droit perdu par l'appelant : la fenêtre reste telle quelle, le droit
   * de l'appelant n'est pas relu comme perdu.
   */
  protectedHolderMessage?: MessageKey;
}

/**
 * Lance une action sur un serveur : `perform` appelle UNE commande typée du pont (la méthode et le
 * chemin sont construits côté Rust ; le bouton qui l'appelle porte `needs-link`, BR-RESIL-008). Le
 * lien tombe avant la réponse (`kind: "unknown"`) : on le dit, puis jamais de rejeu (BR-RESIL-009) ;
 * l'issue arrive plus tard par le store du lien (« Fait pendant la coupure », « Non exécuté. Tu peux
 * relancer. », « Résultat inconnu. », BR-RESIL-010). Aucune fenêtre bloquante : tout passe par des
 * notifications discrètes (BR-RESIL-011). Une action à la fois par appelant (`busy`). Rend `null`
 * si l'action n'a pas pu partir ou a échoué (déjà notifié).
 *
 * `serverId` : le serveur visé par les actions de ce composable. Il est OBLIGATOIRE (C2 de la review) : c'est lui
 * dont on relit le droit « gérer les services » quand le serveur refuse un acte faute de ce droit
 * (`missing_services_right`, BR-GRANT-011). `null` : aucune relecture possible (outil de développement). Le texte
 * du refus est toujours « Tu n'as plus le droit… » : un refus qui atteint ici veut dire que le droit affiché était
 * périmé ; « Tu n'as pas le droit… » (`rights.refused`) ne sort que par `failureMessage`, hors de ce chemin.
 */
export function useServerAction(serverId: () => string | null) {
  const toasts = useToastsStore();
  const busy = ref(false);
  /**
   * La dernière action a été refusée faute du droit de gérer les services (retiré depuis qu'on l'a lu, BR-GRANT-010) :
   * le texte « Tu n'as plus le droit… » est notifié et le droit relu ; une fenêtre ouverte se ferme
   * (`ActReport` `right_lost`).
   */
  const rightLost = ref(false);

  async function run<T extends ActionOutcome>(
    perform: () => Promise<T>,
    options: RunOptions = {},
  ): Promise<T | null> {
    if (busy.value) return null;
    busy.value = true;
    rightLost.value = false;
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
      if (failure?.kind === "missing_services_right" && options.protectedHolderMessage) {
        toasts.push({ kind: "error", message: t(options.protectedHolderMessage) });
        return null;
      }
      if (failure?.kind === "missing_services_right") {
        // Le droit affiché était périmé : on le relit tout de suite, les boutons se désactivent (BR-GRANT-011).
        rightLost.value = true;
        toasts.push({ kind: "error", message: t("rights.lost") });
        const id = serverId();
        if (id) void useServiceRightsStore().noteRefusal(id);
        return null;
      }
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

  /**
   * Le rapport d'une action qui n'a pas abouti (`run` a rendu `null`) : `right_lost` si le serveur l'a refusée faute
   * du droit (la fenêtre ouverte se ferme, le texte est déjà notifié), sinon `failed`.
   */
  function failedReport(): { kind: "right_lost" } | { kind: "failed" } {
    return rightLost.value ? { kind: "right_lost" } : { kind: "failed" };
  }

  return { run, busy, rightLost, failedReport };
}
