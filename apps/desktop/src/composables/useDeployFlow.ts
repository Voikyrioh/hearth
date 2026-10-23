import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import type { RouteLocationRaw } from "vue-router";
import type { DeployFormState } from "@/composables/useDeployForm";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { toRequest } from "@/deploy/form";
import {
  failureField,
  failureText,
  isFieldRefusal,
  refusalField,
  refusalText,
} from "@/deploy/messages";
import { t } from "@/i18n";
import {
  type DeployCheck,
  type DeployField,
  type DeployRefusal,
  getLinkBridge,
  type ServiceInfo,
  type ServiceOperation,
} from "@/link";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";

/** Le refus du serveur montré en tête du formulaire, et le champ qu'il désigne (BR-DEPLOY-032). */
export interface FormRefusal {
  text: string;
  field: DeployField | null;
}

/** Un déploiement lancé depuis cette page : l'opération que l'agent suit, ou « résultat inconnu » (lien coupé). */
export interface Launch {
  operationId: string | null;
  serviceKey: string | null;
  name: string;
  unknown: boolean;
}

/**
 * Le parcours d'un déploiement, de « Déployer » à l'issue (HRT-69, BR-DEPLOY-015, 016, 019, 028, 029, 030, 032) :
 *
 * 1. `openSummary` refait la consultation (le serveur a le dernier mot) : un point qui a changé ramène au champ sans
 *    ouvrir le récapitulatif ; sinon le récapitulatif s'ouvre avec les points d'attention frais ;
 * 2. `perform` envoie le déploiement (acte confirmé, mot de passe toujours demandé). L'agent répond « accepté » :
 *    l'issue ne se lit JAMAIS dans cette réponse, elle arrive par le suivi des opérations (`useServiceOpsStore`) ;
 *    un refus de champ (nom pris, port pris au dernier moment…) ferme la fenêtre et ramène au champ, la saisie reste ;
 *    un résultat inconnu (lien coupé) n'est jamais rejoué ;
 * 3. la progression suit l'opération jusqu'à son issue : réussite et annulation sont dites par une notification et
 *    la page se referme ou reste, échec sans service rouvre le formulaire rempli, service conservé reste dans la liste.
 *
 * La saisie (valeurs de variables, commande) est effacée de la mémoire dès que l'issue est connue et que le formulaire
 * n'a plus à rouvrir, ou aussitôt à l'abandon (BR-DEPLOY-029). Aucun mot de passe n'est gardé : il traverse l'appel.
 */
export function useDeployFlow(options: {
  serverId: () => string;
  form: DeployFormState;
  navigate: (to: RouteLocationRaw) => void;
  /** Met le curseur sur ce champ du formulaire. */
  focusField: (field: DeployField) => void;
}) {
  const { serverId, form } = options;
  const operations = useServiceOpsStore();
  const services = useServicesStore();
  const toasts = useToastsStore();
  const action = useServerAction(() => serverId());

  const summaryOpen = ref(false);
  const summaryCheck = ref<DeployCheck | null>(null);
  /** `undefined` : rien n'est exigé ; sinon l'espace libre que le serveur a dit (`null` : sans chiffre). */
  const forceLowDisk = ref<number | null | undefined>(undefined);
  const opening = ref(false);
  const refusal = ref<FormRefusal | null>(null);
  const launch = ref<Launch | null>(null);
  /** L'opération a pris fin et la fenêtre de progression doit le dire (échec, service conservé, résultat à vérifier). */
  const ended = ref<ServiceOperation | null>(null);

  /**
   * Met le curseur sur un champ APRÈS la fermeture de la fenêtre qui le recouvre : une fenêtre qui se ferme rend le
   * focus à l'élément qui l'avait ouverte, ce qui effacerait ce geste s'il venait avant.
   */
  function focusLater(field: DeployField): void {
    void nextTick()
      .then(() => nextTick())
      .then(() => options.focusField(field));
  }

  const spaceRefusal = (r: DeployRefusal) => (r.kind === "no_space" ? r.freeBytes : undefined);

  // ── 1. Le récapitulatif ──────────────────────────────────────────────────────────────────────

  /** Ce que la consultation fraîche interdit, ou `null` : le serveur d'aujourd'hui est d'accord avec le formulaire. */
  function blockingOf(check: DeployCheck): FormRefusal | null {
    if (check.name === "taken") return { text: t("deploy.nameTaken"), field: "name" };
    if (check.name === "being_deleted") {
      return { text: t("deploy.nameBeingDeleted"), field: "name" };
    }
    for (const port of check.ports) {
      if (port.status === "free" || port.status === "reserved_by_stopped") continue;
      return {
        text: refusalText({
          kind: "port_in_use",
          port: port.hostPort,
          protocol: port.protocol,
          status: port.status,
          holder: port.holder,
          suggested: port.suggested,
        }),
        field: "ports",
      };
    }
    return null;
  }

  async function openSummary(): Promise<void> {
    if (opening.value || summaryOpen.value) return;
    form.touchAll();
    if (form.issues.value.any) return;
    opening.value = true;
    refusal.value = null;
    forceLowDisk.value = undefined;
    try {
      const fresh = await form.refreshCheck();
      const blocking = fresh ? blockingOf(fresh) : null;
      if (blocking) {
        refusal.value = blocking;
        if (blocking.field) focusLater(blocking.field);
        return;
      }
      summaryCheck.value = fresh;
      summaryOpen.value = true;
    } finally {
      opening.value = false;
    }
  }

  function closeSummary(): void {
    summaryOpen.value = false;
  }

  // ── 2. L'envoi ───────────────────────────────────────────────────────────────────────────────

  async function perform(
    adminPassword: string | null,
    accept: { reservedPorts: boolean; lowDisk: boolean },
  ): Promise<ActReport> {
    const request = toRequest(form.draft.value, accept);
    if (!request) return { kind: "failed" };
    const id = serverId();
    const result = await action.run(
      () => getLinkBridge().deployService(id, request, adminPassword),
      { unknownMessage: "deploy.unknownResult", forbiddenMessage: "rights.refused" },
    );
    if (!result) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (result.kind === "unknown") {
      launch.value = { operationId: null, serviceKey: null, name: request.name, unknown: true };
      return { kind: "unknown" };
    }
    if (result.kind === "accepted") {
      // La fenêtre de progression suit l'opération elle-même : son issue ne passe pas par une notification.
      operations.watch(id, result.operationId);
      launch.value = {
        operationId: result.operationId,
        serviceKey: result.serviceKey,
        name: request.name,
        unknown: false,
      };
      ended.value = null;
      refusal.value = null;
      return { kind: "done" };
    }
    return settleRefusal(result.refusal);
  }

  /** Un refus de l'agent : celui qui vise un champ ferme la fenêtre et y ramène ; les autres se disent dans la fenêtre. */
  function settleRefusal(refused: DeployRefusal): ActReport {
    const free = spaceRefusal(refused);
    if (free !== undefined) forceLowDisk.value = free;
    if (!isFieldRefusal(refused)) return { kind: "refused", refusal: refused };
    const field = refusalField(refused);
    refusal.value = { text: refusalText(refused), field };
    // Les constats du formulaire se rafraîchissent : le port refusé propose son port libre, le nom se dit pris.
    void form.refreshCheck();
    summaryOpen.value = false;
    if (field) focusLater(field);
    return { kind: "done" };
  }

  // ── 3. La progression ────────────────────────────────────────────────────────────────────────

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
  /** Le service tel qu'il est dans la liste, quand il y est (code de sortie, boucle). */
  const service = computed<ServiceInfo | null>(() => {
    const key = launch.value?.serviceKey;
    if (!key) return null;
    return services.of(serverId())?.services.find((item) => item.key === key) ?? null;
  });

  function settle(): void {
    const current = launch.value;
    if (current?.operationId) operations.settle(serverId(), current.operationId);
  }

  watch(finished, (done) => {
    const current = launch.value;
    if (!done || !current) return;
    const op = done.operation;
    const key = current.serviceKey;
    if (op.status === "succeeded" && op.cause !== "stopped_right_after_start") {
      toasts.push({ kind: "success", message: t("deploy.success", { name: current.name }) });
      settle();
      form.wipe();
      launch.value = null;
      if (key) {
        options.navigate({
          name: "services",
          params: { id: serverId() },
          query: { service: key },
        });
      }
      return;
    }
    if (op.status === "cancelled") {
      // Rien n'a été créé : la notification le dit, le formulaire reste rempli (BR-DEPLOY-016).
      toasts.push({ kind: "info", message: t("deploy.cancelled") });
      settle();
      launch.value = null;
      return;
    }
    if (op.status === "succeeded" || op.status === "interrupted") {
      // Un service existe, ou son sort est inconnu : on ne rouvre rien, la saisie est effacée.
      form.wipe();
    }
    ended.value = op;
  });

  /** « Continuer en arrière-plan » : le déploiement continue, la liste porte sa progression (BR-DEPLOY-024). */
  function background(): void {
    const current = launch.value;
    if (current?.operationId) operations.release(serverId(), current.operationId);
    form.wipe();
    launch.value = null;
    options.navigate({ name: "services", params: { id: serverId() } });
  }

  /** Rien n'a été créé : le formulaire rouvre rempli, le champ concerné est signalé. */
  function modify(): void {
    const done = ended.value;
    settle();
    launch.value = null;
    ended.value = null;
    if (!done) return;
    const field = failureField(done);
    refusal.value = { text: failureText(done), field };
    if (field) focusLater(field);
  }

  /** « Fermer » : l'issue est dite, la page revient à la liste (sur le service s'il existe). */
  function close(): void {
    const current = launch.value;
    const done = ended.value;
    settle();
    launch.value = null;
    ended.value = null;
    form.wipe();
    const key = done?.status === "succeeded" ? (current?.serviceKey ?? null) : null;
    options.navigate({
      name: "services",
      params: { id: serverId() },
      query: key ? { service: key } : {},
    });
  }

  /** « Voir les journaux » du service conservé. */
  function seeLogs(): void {
    const key = launch.value?.serviceKey;
    settle();
    launch.value = null;
    ended.value = null;
    form.wipe();
    options.navigate({
      name: "services",
      params: { id: serverId() },
      query: key ? { service: key, onglet: "journaux" } : {},
    });
  }

  /** Annule la récupération (acte confirmé, couvert par l'élévation de 5 minutes). */
  async function performCancel(adminPassword: string | null): Promise<ActReport> {
    const current = launch.value;
    if (!current?.operationId) return { kind: "failed" };
    const id = serverId();
    const opId = current.operationId;
    const result = await action.run(
      () => getLinkBridge().cancelServiceOperation(id, opId, adminPassword),
      { unknownMessage: "deploy.unknownResult", forbiddenMessage: "rights.refused" },
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

  // La page se ferme avec une fenêtre de progression ouverte : l'issue sera annoncée comme les autres.
  onBeforeUnmount(() => {
    const current = launch.value;
    if (current?.operationId) operations.release(serverId(), current.operationId);
  });

  return {
    summaryOpen,
    summaryCheck,
    forceLowDisk,
    opening,
    refusal,
    launch,
    ended,
    operation,
    service,
    busy: action.busy,
    openSummary,
    closeSummary,
    perform,
    performCancel,
    background,
    modify,
    close,
    seeLogs,
  };
}

export type DeployFlow = ReturnType<typeof useDeployFlow>;
