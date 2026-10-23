import { computed, ref } from "vue";
import { useLifecycleActions } from "@/composables/useLifecycleActions";
import { type ActReport, useReauth } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import { alreadyText, type Gesture, refusalText } from "@/services/lifecycle";
import { useToastsStore } from "@/stores/toasts";

/** Le geste en attente de confirmation : le service visé (par sa clé stable) et ce que l'on veut en faire. */
export interface PendingGesture {
  /** Le service tel qu'il était à l'ouverture : de quoi garder son nom si la liste le perd (BR-LIFE-022). */
  service: ServiceInfo;
  gesture: Gesture;
}

/**
 * Le parcours de pilotage d'un service, tenu par la page (HRT-65, BR-LIFE-007, 016, 017, 022) :
 *
 * - arrêter et redémarrer ouvrent TOUJOURS la fenêtre de confirmation (l'effet, les avertissements du service, puis
 *   le mot de passe seulement si l'agent le demande : jamais en état normal, HRT-194) ;
 * - démarrer n'ouvre la fenêtre que si l'agent a quelque chose à demander (mot de passe) ; sinon, en état normal
 *   avec la clé de ce poste, l'ordre part tout de suite ;
 * - annuler la fenêtre ne change rien et n'envoie rien (BR-LIFE-017) ;
 * - l'agent répond « accepté » : l'issue arrive par le suivi des opérations (HRT-84), jamais ici.
 *
 * Le service est suivi par sa CLÉ dans la liste vivante : si son état change sous la fenêtre, la fenêtre le dit.
 */
export function useServiceLifecycle(
  serverId: () => string,
  services: () => readonly ServiceInfo[],
) {
  const toasts = useToastsStore();
  const actions = useLifecycleActions(serverId);
  /** Le dernier geste demandé : la fenêtre garde son sujet le temps de se fermer. */
  const pending = ref<PendingGesture | null>(null);
  const open = ref(false);
  // La lecture de l'état de confirmation pour un démarrage direct (jamais gardée au-delà de l'appel).
  const startCheck = useReauth(serverId, () => "service_start");
  const starting = ref(false);

  /** Le service visé, tel qu'il est maintenant dans la liste ; `null` : il a disparu. */
  const current = computed<ServiceInfo | null>(() =>
    pending.value
      ? (services().find((service) => service.key === pending.value?.service.key) ?? null)
      : null,
  );

  /** Les refus qui ne demandent aucun geste de plus : un message, et la fenêtre se ferme (BR-LIFE-014, 015). */
  function settle(report: ActReport, service: ServiceInfo): ActReport {
    if (report.kind !== "refused" || !report.refusal) return report;
    switch (report.refusal.kind) {
      case "already_in_state": {
        const refusal = report.refusal as { state?: ServiceInfo["state"] | null };
        toasts.push({ kind: "info", message: alreadyText(refusal.state, service) });
        return { kind: "done" };
      }
      case "service_not_found":
        toasts.push({ kind: "error", message: refusalText(report.refusal, service) });
        return { kind: "done" };
      default:
        return report;
    }
  }

  /** Lance l'acte (appelé par la fenêtre, avec le mot de passe saisi ou `null` sous l'élévation). */
  async function perform(adminPassword: string | null): Promise<ActReport> {
    const target = pending.value;
    if (!target || !open.value) return { kind: "failed" };
    const report = await actions.perform(target.service.key, target.gesture, adminPassword);
    return settle(report, target.service);
  }

  /**
   * Demande un geste. Arrêter et redémarrer : la fenêtre s'ouvre. Démarrer : la fenêtre ne s'ouvre que si l'agent
   * demande quelque chose (BR-LIFE-016, grille de confiance HRT-194), sinon l'ordre part directement.
   */
  async function request(service: ServiceInfo, gesture: Gesture): Promise<void> {
    if (gesture !== "start") {
      show(service, gesture);
      return;
    }
    if (starting.value) return;
    starting.value = true;
    try {
      await startCheck.load();
      const nothingToAsk =
        startCheck.ready.value &&
        startCheck.supported.value &&
        !startCheck.keyMissing.value &&
        !startCheck.needsPassword.value;
      if (!nothingToAsk) {
        show(service, gesture);
        return;
      }
      const report = settle(await actions.perform(service.key, "start", null), service);
      // L'état de confiance a changé entre la lecture et l'ordre (normal vers alerte, ou délai fermé) : la fenêtre
      // s'ouvre avec le champ, le geste n'est pas perdu.
      if (report.kind === "refused" && report.refusal?.kind === "password_required") {
        show(service, gesture);
        return;
      }
      if (report.kind === "refused" && report.refusal) {
        toasts.push({ kind: "error", message: startRefusal(report.refusal, service) });
      }
    } finally {
      starting.value = false;
    }
  }

  function startRefusal(refusal: { kind: string }, service: ServiceInfo): string {
    if (refusal.kind === "wrong_password" || refusal.kind === "too_many_attempts") {
      return t("lifecycle.refusedOther");
    }
    return refusalText(refusal, service);
  }

  function show(service: ServiceInfo, gesture: Gesture): void {
    pending.value = { service, gesture };
    open.value = true;
  }

  /** Ferme la fenêtre sans rien envoyer ni rien inscrire (BR-LIFE-017). */
  function cancel(): void {
    open.value = false;
  }

  return {
    pending,
    current,
    open,
    busy: computed(() => actions.busy.value || starting.value),
    request,
    perform,
    cancel,
  };
}
