import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type DeletePreview,
  type DeletePreviewRefusal,
  type DeleteRequest,
  failureOf,
  getLinkBridge,
  type ServiceInfo,
  type ServiceOperation,
} from "@/link";
import { outcomeOf } from "@/serviceOps/messages";
import { erasableFolders } from "@/services/delete";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useToastsStore } from "@/stores/toasts";

/** Ce que la consultation de la fenêtre a donné (BR-DEL-009 : la mesure des dossiers peut durer 10 secondes). */
export type PreviewState =
  | { kind: "loading" }
  | { kind: "ready"; preview: DeletePreview }
  | { kind: "refused"; refusal: DeletePreviewRefusal }
  | { kind: "failed" };

/** La suppression lancée, telle que la fenêtre de suivi la suit. */
export interface DeleteLaunch {
  /** `null` : le lien est tombé avant la réponse, le résultat n'est pas connu (BR-DEL-023). */
  operationId: string | null;
  name: string;
  /** Le service tournait : l'arrêt est une étape du plan. */
  stop: boolean;
  /** « Supprimer aussi les données » était cochée et au moins un dossier pouvait partir. */
  erase: boolean;
  /** Les dossiers gérés que la suppression pouvait effacer, tels que la fenêtre les a montrés. */
  folders: string[];
  unknown: boolean;
}

const OPTIONS = {
  unknownMessage: "serviceDelete.unknownResult",
  forbiddenMessage: "serviceDelete.forbidden",
  passwordMissingIsRefusal: true,
} as const;

/**
 * Le parcours de suppression d'un service, tenu par la page (HRT-86, BR-DEL-009 à 014, 021 à 023, 026, 031) :
 *
 * - « Supprimer ce service » ouvre la fenêtre de confirmation et lit la prévisualisation des dossiers (une
 *   consultation : rien n'est écrit) ;
 * - la confirmation est un acte jamais couvert par l'élévation : le mot de passe est demandé à chaque fois ;
 * - l'agent répond « accepté » : la fenêtre devient une vue de suivi qui suit l'opération elle-même (son issue ne
 *   passe pas par une notification tant qu'elle est ouverte) ; l'issue arrive par le suivi des opérations (HRT-84) ;
 * - annuler est possible jusqu'à la suppression du conteneur, jamais après ; fermer la fenêtre n'arrête rien.
 *
 * Le service visé est suivi par sa CLÉ dans la liste vivante : s'il change sous la fenêtre, la fenêtre le dit.
 */
export function useServiceDelete(serverId: () => string, services: () => readonly ServiceInfo[]) {
  const toasts = useToastsStore();
  const operations = useServiceOpsStore();
  const action = useServerAction(serverId);
  /** Le service visé tel qu'il était à l'ouverture : de quoi garder son nom si la liste le perd. */
  const target = ref<ServiceInfo | null>(null);
  const phase = ref<"closed" | "confirm" | "progress">("closed");
  const preview = ref<PreviewState>({ kind: "loading" });
  const launch = ref<DeleteLaunch | null>(null);
  /** L'opération a pris fin et la fenêtre de suivi doit le dire. */
  const ended = ref<ServiceOperation | null>(null);
  let reading = 0;

  /** Le service visé, tel qu'il est maintenant dans la liste ; `null` : il a disparu. */
  const current = computed<ServiceInfo | null>(() =>
    target.value ? (services().find((service) => service.key === target.value?.key) ?? null) : null,
  );

  async function loadPreview(): Promise<void> {
    const service = target.value;
    if (!service) return;
    const mine = ++reading;
    preview.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readDeletePreview(serverId(), service.key);
      if (mine !== reading) return;
      preview.value =
        outcome.kind === "ready"
          ? { kind: "ready", preview: outcome.preview }
          : { kind: "refused", refusal: outcome.refusal };
    } catch (error) {
      if (mine !== reading) return;
      const failure = failureOf(error);
      if (failure?.kind === "missing_services_right") {
        // Le droit affiché était périmé : on le relit, la fenêtre se ferme (BR-DEL-027).
        toasts.push({ kind: "error", message: t("rights.lost") });
        void useServiceRightsStore().noteRefusal(serverId());
        reset();
        return;
      }
      if (!failure) logUiError(error, "serviceDelete:preview");
      preview.value = { kind: "failed" };
    }
  }

  /** Ouvre la fenêtre pour ce service et lit les dossiers. */
  function request(service: ServiceInfo): void {
    if (phase.value !== "closed") return;
    target.value = service;
    launch.value = null;
    ended.value = null;
    phase.value = "confirm";
    void loadPreview();
  }

  /**
   * Ferme tout. Le service visé et la dernière suppression restent tenus (les fenêtres restent montées, fermées : le
   * focus revient à leur ouverture) ; `request` les remplace à la demande suivante.
   */
  function reset(): void {
    reading += 1;
    phase.value = "closed";
  }

  /** « Annuler » : ne change rien et n'envoie rien (BR-LIFE-017). */
  function closeConfirm(): void {
    if (phase.value === "confirm") reset();
  }

  /** Lance l'acte (appelé par la fenêtre, avec le mot de passe saisi) : jamais rejoué (BR-DEL-023). */
  async function perform(request: DeleteRequest, adminPassword: string | null): Promise<ActReport> {
    const service = target.value;
    const shown = preview.value.kind === "ready" ? preview.value.preview : null;
    if (!service || !shown || phase.value !== "confirm") return { kind: "failed" };
    const id = serverId();
    const result = await action.run(
      () => getLinkBridge().deleteService(id, service.key, request, adminPassword),
      OPTIONS,
    );
    if (!result) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    const folders = erasableFolders(shown).map((folder) => folder.path);
    const made = {
      name: shown.name,
      stop: shown.running,
      erase: request.eraseData && folders.length > 0,
      folders,
    };
    if (result.kind === "unknown") {
      launch.value = { ...made, operationId: null, unknown: true };
      phase.value = "progress";
      return { kind: "unknown" };
    }
    if (result.kind === "accepted") {
      // La fenêtre de suivi suit l'opération elle-même : son issue ne passe pas par une notification.
      operations.watch(id, result.operationId);
      launch.value = { ...made, operationId: result.operationId, unknown: false };
      ended.value = null;
      phase.value = "progress";
      return { kind: "done" };
    }
    const refusal = result.refusal;
    // Le service n'existe plus ou est adopté : rien à demander de plus, un message et la fenêtre se ferme (BR-DEL-002, 018).
    if (refusal.kind === "service_not_found" || refusal.kind === "service_adopted") {
      toasts.push({
        kind: "error",
        message: t(
          refusal.kind === "service_not_found"
            ? "serviceDelete.refusedGone"
            : "serviceDelete.refusedAdopted",
        ),
      });
      return { kind: "done" };
    }
    return { kind: "refused", refusal };
  }

  // ── Le suivi ─────────────────────────────────────────────────────────────────────────────────

  const tracked = computed(() => {
    const current = launch.value;
    return current?.operationId ? operations.get(serverId(), current.operationId) : undefined;
  });
  /** L'opération telle qu'elle court (étape, progression, annulable). */
  const operation = computed<ServiceOperation | null>(() => tracked.value?.operation ?? null);
  const finished = computed(() => {
    const current = launch.value;
    return current?.operationId
      ? operations.finishedOf(serverId(), current.operationId)
      : undefined;
  });

  watch(finished, (done) => {
    const current = launch.value;
    if (!done || !current) return;
    ended.value = done.operation;
    // Succès et annulation sont dits aussi par une notification (BR-DEL-020) ; l'issue qui demande un mot se dit dans la fenêtre.
    if (!done.announced) {
      const outcome = outcomeOf(done.operation, current.name);
      if (
        outcome &&
        (done.operation.status === "succeeded" || done.operation.status === "cancelled")
      ) {
        toasts.push({ kind: outcome.tone, message: outcome.message });
      }
    }
  });

  function settle(): void {
    const current = launch.value;
    if (current?.operationId) operations.settle(serverId(), current.operationId);
  }

  /** « Fermer » : l'issue est dite ; ou la suppression continue en arrière-plan, la bande la porte (BR-DEL-031). */
  function closeProgress(): void {
    const current = launch.value;
    if (current?.operationId) {
      if (ended.value) settle();
      else operations.release(serverId(), current.operationId);
    }
    reset();
  }

  /** Annule avant le point de non-retour (acte confirmé, couvert par l'élévation de 5 minutes). */
  async function performCancel(adminPassword: string | null): Promise<ActReport> {
    const current = launch.value;
    if (!current?.operationId) return { kind: "failed" };
    const id = serverId();
    const opId = current.operationId;
    const result = await action.run(
      () => getLinkBridge().cancelServiceOperation(id, opId, adminPassword),
      OPTIONS,
    );
    if (!result) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (result.kind === "done") return { kind: "done" };
    if (result.kind === "unknown") return { kind: "unknown" };
    return { kind: "refused", refusal: result.refusal };
  }

  // La page se ferme avec une fenêtre de suivi ouverte : l'issue sera annoncée comme les autres.
  onBeforeUnmount(() => {
    const current = launch.value;
    if (current?.operationId) operations.release(serverId(), current.operationId);
  });

  return {
    target,
    phase,
    preview,
    launch,
    ended,
    current,
    operation,
    busy: action.busy,
    request,
    loadPreview,
    closeConfirm,
    perform,
    performCancel,
    closeProgress,
  };
}

export type ServiceDeleteFlow = ReturnType<typeof useServiceDelete>;
