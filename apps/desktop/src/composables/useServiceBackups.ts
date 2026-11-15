import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useOperationFollow } from "@/composables/useOperationFollow";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  type BackupInfo,
  type BackupPreview,
  type BackupReadRefusal,
  type BackupRequest,
  type BackupSettings,
  type BackupsView,
  failureOf,
  getLinkBridge,
  type ServiceInfo,
  type ServiceOperation,
} from "@/link";
import { dateOf } from "@/services/backups";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useToastsStore } from "@/stores/toasts";

/** Ce que la lecture de la liste a donné (BR-BACKUP-012, 028). */
export type ListState =
  | { kind: "loading" }
  | { kind: "ready"; view: BackupsView }
  | { kind: "refused"; refusal: BackupReadRefusal }
  | { kind: "failed" };

/** Ce que l'estimation de la fenêtre de création a donné (BR-BACKUP-009 : jusqu'à 60 secondes). */
export type PreviewState =
  | { kind: "loading" }
  | { kind: "ready"; preview: BackupPreview }
  | { kind: "refused"; refusal: BackupReadRefusal }
  | { kind: "failed" };

/** Le résultat d'une vérification, dit dans la ligne de la sauvegarde. */
export type VerifyNote =
  | { kind: "ok"; files: number }
  | { kind: "corrupt" }
  | { kind: "missing" }
  | { kind: "in-use" }
  | { kind: "failed" }
  | { kind: "unknown" };

/** L'issue d'une sauvegarde lancée depuis cet onglet, dite en haut de l'onglet jusqu'à sa fermeture. */
export interface BackupResult {
  operation: ServiceOperation;
  name: string;
  backupId: string;
  /** La sauvegarde créée, relue dans la liste (elle porte la date, la taille et les repères). */
  backup: BackupInfo | null;
}

const OPTIONS = {
  unknownMessage: "backups.unknownResult",
  forbiddenMessage: "backups.refused.other",
  passwordMissingIsRefusal: true,
} as const;

/**
 * Le parcours de l'onglet « Sauvegardes » d'un service (HRT-98, BR-BACKUP-001 à 015, 026 à 028) :
 *
 * - la liste est lue à l'ouverture, relue au retour du lien et à la fin de toute opération de sauvegarde sur ce service ;
 * - « Sauvegarder » ouvre la fenêtre de création et lit l'estimation (une consultation : rien n'est écrit) ; la
 *   confirmation est un acte couvert par l'élévation ; l'agent répond « accepté » : l'onglet suit l'opération lui-même
 *   (son issue ne passe pas par une notification tant qu'il est ouvert) ;
 * - « Vérifier » est une consultation, dite dans la ligne ; « Supprimer » est un acte jamais couvert par l'élévation ;
 * - les éléments ignorés sont réglés par un acte couvert par l'élévation.
 *
 * Le service est suivi par sa CLÉ ; la note d'un compte n'est écrite dans aucune trace.
 */
export function useServiceBackups(
  serverId: () => string,
  service: () => ServiceInfo,
  connected: () => boolean,
) {
  const toasts = useToastsStore();
  const operations = useServiceOpsStore();
  const action = useServerAction(serverId);

  const list = ref<ListState>({ kind: "loading" });
  /** Une relecture est en cours (la liste précédente reste affichée). */
  const refreshing = ref(false);
  let reading = 0;

  function noteLostRight(): void {
    toasts.push({ kind: "error", message: t("rights.lost") });
    void useServiceRightsStore().noteRefusal(serverId());
  }

  /** Lit la liste ; une relecture qui échoue garde la liste déjà lue (datée par la page, jamais effacée). */
  async function load(): Promise<void> {
    const mine = ++reading;
    const had = list.value.kind === "ready";
    if (had) refreshing.value = true;
    else list.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readBackups(serverId(), service().key);
      if (mine !== reading) return;
      list.value =
        outcome.kind === "ready"
          ? { kind: "ready", view: outcome.view }
          : { kind: "refused", refusal: outcome.refusal };
    } catch (error) {
      if (mine !== reading) return;
      if (!failureOf(error)) logUiError(error, "serviceBackups:list");
      if (!had) list.value = { kind: "failed" };
    } finally {
      if (mine === reading) refreshing.value = false;
    }
  }

  void load();
  watch(connected, (now, before) => {
    if (now && !before) void load();
  });
  // Une autre sauvegarde (d'un autre compte, d'une autre fenêtre) change la liste en finissant.
  const ongoing = computed(() => operations.forService(serverId(), service().key));
  const ongoingBackup = computed(() =>
    ongoing.value?.operation.kind === "backup" ? ongoing.value : undefined,
  );
  watch(ongoingBackup, (now, before) => {
    if (!now && before) void load();
  });

  // ── La fenêtre de création ──────────────────────────────────────────────────────────────────

  const createOpen = ref(false);
  const preview = ref<PreviewState>({ kind: "loading" });
  /** Le résultat de la dernière demande de création est inconnu : le lien est tombé avant la réponse. */
  const unknownResult = ref(false);
  let previewing = 0;

  async function loadPreview(): Promise<void> {
    const mine = ++previewing;
    preview.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readBackupPreview(serverId(), service().key);
      if (mine !== previewing) return;
      preview.value =
        outcome.kind === "ready"
          ? { kind: "ready", preview: outcome.preview }
          : { kind: "refused", refusal: outcome.refusal };
    } catch (error) {
      if (mine !== previewing) return;
      const failure = failureOf(error);
      if (failure?.kind === "missing_services_right") {
        noteLostRight();
        closeCreate();
        return;
      }
      if (!failure) logUiError(error, "serviceBackups:preview");
      preview.value = { kind: "failed" };
    }
  }

  function openCreate(): void {
    if (createOpen.value) return;
    unknownResult.value = false;
    createOpen.value = true;
    void loadPreview();
  }

  function closeCreate(): void {
    previewing += 1;
    createOpen.value = false;
  }

  // ── L'opération lancée et son issue ─────────────────────────────────────────────────────────

  const launch = ref<{ operationId: string; backupId: string; name: string } | null>(null);
  const follow = useOperationFollow(serverId, () => launch.value?.operationId ?? null);
  const result = ref<BackupResult | null>(null);

  watch(follow.finished, (done) => {
    const current = launch.value;
    if (!done || !current) return;
    result.value = {
      operation: done.operation,
      name: current.name,
      backupId: current.backupId,
      backup: null,
    };
    void load().then(() => {
      if (!result.value || list.value.kind !== "ready") return;
      const found = list.value.view.backups.find((item) => item.id === current.backupId) ?? null;
      result.value = { ...result.value, backup: found };
    });
  });

  /** Ferme le résultat : l'issue est dite, acquittée auprès de la coquille (jamais rejouée). */
  function dismissResult(): void {
    follow.settle();
    result.value = null;
    launch.value = null;
  }

  /** Lance la création (appelé par la fenêtre, avec le mot de passe saisi) : jamais rejoué. */
  async function performCreate(
    request: BackupRequest,
    adminPassword: string | null,
  ): Promise<ActReport> {
    const shown = preview.value.kind === "ready" ? preview.value.preview : null;
    if (!shown || !createOpen.value) return { kind: "failed" };
    const id = serverId();
    const key = service().key;
    const outcome = await action.run(
      () => getLinkBridge().createBackup(id, key, request, adminPassword),
      OPTIONS,
    );
    if (!outcome) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (outcome.kind === "unknown") {
      unknownResult.value = true;
      closeCreate();
      return { kind: "unknown" };
    }
    if (outcome.kind === "accepted") {
      // Une issue d'un lancement précédent encore affichée est remplacée.
      if (result.value) dismissResult();
      launch.value = {
        operationId: outcome.operationId,
        backupId: outcome.backupId,
        name: shown.name,
      };
      follow.begin(outcome.operationId);
      closeCreate();
      return { kind: "done" };
    }
    const refusal = outcome.refusal;
    // Le service n'existe plus : rien à demander de plus, un message et la fenêtre se ferme.
    if (refusal.kind === "service_not_found" || refusal.kind === "service_adopted") {
      toasts.push({
        kind: "error",
        message: t(
          refusal.kind === "service_not_found" ? "backups.refused.gone" : "backups.refused.adopted",
        ),
      });
      closeCreate();
      return { kind: "done" };
    }
    // L'espace a changé depuis l'estimation : la fenêtre relit ses chiffres sans perdre la saisie.
    if (refusal.kind === "no_space") void loadPreview();
    return { kind: "refused", refusal };
  }

  // ── Annuler la sauvegarde qui court ─────────────────────────────────────────────────────────

  const cancelOpen = ref(false);

  async function performCancel(adminPassword: string | null): Promise<ActReport> {
    const current = ongoingBackup.value;
    if (!current) return { kind: "done" };
    const id = serverId();
    const opId = current.operation.id;
    const outcome = await action.run(
      () => getLinkBridge().cancelServiceOperation(id, opId, adminPassword),
      OPTIONS,
    );
    if (!outcome) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (outcome.kind === "done") return { kind: "done" };
    if (outcome.kind === "unknown") return { kind: "unknown" };
    return { kind: "refused", refusal: outcome.refusal };
  }

  // ── Vérifier ────────────────────────────────────────────────────────────────────────────────

  const verifying = ref<ReadonlySet<string>>(new Set());
  const verifyNotes = ref<ReadonlyMap<string, VerifyNote>>(new Map());

  function setNote(id: string, note: VerifyNote | null): void {
    const next = new Map(verifyNotes.value);
    if (note === null) next.delete(id);
    else next.set(id, note);
    verifyNotes.value = next;
  }

  async function verify(backup: BackupInfo): Promise<void> {
    if (verifying.value.has(backup.id)) return;
    verifying.value = new Set(verifying.value).add(backup.id);
    setNote(backup.id, null);
    try {
      const outcome = await getLinkBridge().verifyBackup(serverId(), backup.id);
      if (outcome.kind === "done") {
        const { integrity, verifiedAt, filesChecked } = outcome.result;
        if (list.value.kind === "ready") {
          list.value = {
            kind: "ready",
            view: {
              ...list.value.view,
              backups: list.value.view.backups.map((item) =>
                item.id === backup.id ? { ...item, integrity, verifiedAt } : item,
              ),
            },
          };
        }
        setNote(
          backup.id,
          integrity === "verified"
            ? { kind: "ok", files: filesChecked }
            : integrity === "missing"
              ? { kind: "missing" }
              : { kind: "corrupt" },
        );
      } else if (outcome.refusal.kind === "backup_in_use") {
        setNote(backup.id, { kind: "in-use" });
      } else if (outcome.refusal.kind === "backup_not_found") {
        toasts.push({ kind: "info", message: t("backups.refused.notFound") });
        void load();
      } else {
        setNote(backup.id, { kind: "failed" });
      }
    } catch (error) {
      const failure = failureOf(error);
      if (failure?.kind === "missing_services_right") {
        noteLostRight();
      } else if (failure?.kind === "not_connected") {
        setNote(backup.id, { kind: "unknown" });
      } else {
        if (!failure) logUiError(error, "serviceBackups:verify");
        setNote(backup.id, { kind: "failed" });
      }
    } finally {
      const next = new Set(verifying.value);
      next.delete(backup.id);
      verifying.value = next;
    }
  }

  // ── Supprimer ───────────────────────────────────────────────────────────────────────────────

  const deleting = ref<BackupInfo | null>(null);

  function requestDelete(backup: BackupInfo): void {
    deleting.value = backup;
  }

  function closeDelete(): void {
    deleting.value = null;
  }

  async function performDelete(adminPassword: string | null): Promise<ActReport> {
    const target = deleting.value;
    if (!target) return { kind: "failed" };
    const id = serverId();
    const outcome = await action.run(
      () => getLinkBridge().deleteBackup(id, target.id, adminPassword),
      OPTIONS,
    );
    if (!outcome) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (outcome.kind === "unknown") {
      void load();
      return { kind: "unknown" };
    }
    if (outcome.kind === "done") {
      toasts.push({
        kind: "success",
        message: t(outcome.removedFiles ? "backups.deleted" : "backups.deletedRowOnly", {
          date: dateOf(target.createdAt),
        }),
      });
      setNote(target.id, null);
      void load();
      return { kind: "done" };
    }
    // Déjà disparue : la liste se relit, rien d'autre à faire.
    if (outcome.refusal.kind === "backup_not_found") {
      toasts.push({ kind: "info", message: t("backups.refused.notFound") });
      void load();
      return { kind: "done" };
    }
    return { kind: "refused", refusal: outcome.refusal };
  }

  // ── Les éléments ignorés ────────────────────────────────────────────────────────────────────

  const ignoredOpen = ref(false);
  /** L'estimation lue à l'ouverture de la fenêtre : elle porte les éléments ignorés par défaut. */
  const ignoredPreview = ref<PreviewState>({ kind: "loading" });
  let readingIgnored = 0;

  async function openIgnored(): Promise<void> {
    if (ignoredOpen.value) return;
    ignoredOpen.value = true;
    const mine = ++readingIgnored;
    ignoredPreview.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readBackupPreview(serverId(), service().key);
      if (mine !== readingIgnored) return;
      ignoredPreview.value =
        outcome.kind === "ready"
          ? { kind: "ready", preview: outcome.preview }
          : { kind: "refused", refusal: outcome.refusal };
    } catch (error) {
      if (mine !== readingIgnored) return;
      const failure = failureOf(error);
      if (failure?.kind === "missing_services_right") {
        noteLostRight();
        closeIgnored();
        return;
      }
      if (!failure) logUiError(error, "serviceBackups:ignored");
      ignoredPreview.value = { kind: "failed" };
    }
  }

  function closeIgnored(): void {
    readingIgnored += 1;
    ignoredOpen.value = false;
  }

  async function performSaveSettings(
    settings: BackupSettings,
    adminPassword: string | null,
  ): Promise<ActReport> {
    const id = serverId();
    const key = service().key;
    const outcome = await action.run(
      () => getLinkBridge().saveBackupSettings(id, key, settings, adminPassword),
      OPTIONS,
    );
    if (!outcome) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (outcome.kind === "unknown") {
      void load();
      return { kind: "unknown" };
    }
    if (outcome.kind === "done") {
      toasts.push({ kind: "success", message: t("backups.settingsSaved") });
      void load();
      return { kind: "done" };
    }
    return { kind: "refused", refusal: outcome.refusal };
  }

  onBeforeUnmount(() => {
    reading += 1;
    previewing += 1;
    readingIgnored += 1;
  });

  return {
    list,
    refreshing,
    load,
    ongoing,
    ongoingBackup,
    createOpen,
    preview,
    unknownResult,
    openCreate,
    closeCreate,
    loadPreview,
    performCreate,
    result,
    dismissResult,
    cancelOpen,
    performCancel,
    verifying,
    verifyNotes,
    verify,
    deleting,
    requestDelete,
    closeDelete,
    performDelete,
    ignoredOpen,
    ignoredPreview,
    openIgnored,
    closeIgnored,
    performSaveSettings,
    busy: action.busy,
  };
}

export type ServiceBackupsFlow = ReturnType<typeof useServiceBackups>;
