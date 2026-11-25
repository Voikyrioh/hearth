import { computed, ref, watch } from "vue";
import { useOperationFollow } from "@/composables/useOperationFollow";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import type { PendingAdd } from "@/composables/useServiceMods";
import { logUiError } from "@/errors/report";
import {
  type BackupInfo,
  type BackupPreview,
  failureOf,
  getLinkBridge,
  type ModsBatch,
  type OperationTail,
  type ServiceInfo,
} from "@/link";
import { applyRefusalText, type ChangeRow, outcomeOf } from "@/services/modsApply";

/** Ce que l'estimation de la sauvegarde d'avant a donné (BR-MOD-026). */
export type PreviewState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; preview: BackupPreview }
  | { kind: "failed" };

/** L'opération lancée d'ici : ce qu'elle change, depuis quel état de service, avec quelle sauvegarde. */
export interface Launch {
  operationId: string;
  backupId: string;
  rows: ChangeRow[];
  count: number;
  /** Le serveur tournait : l'opération l'arrête et le relance (BR-MOD-025). */
  running: boolean;
  name: string;
}

/**
 * Appliquer un lot de mods (HRT-263, `BR-MOD-025` à `027`, `031`) : l'estimation de la sauvegarde d'avant (une
 * consultation, rien n'est écrit), l'acte `ModsApply` (le mot de passe à chaque fois, jamais couvert par l'élévation,
 * jamais rejoué), le suivi de l'opération par `useOperationFollow` (quittable : sans suivi, l'issue est annoncée comme les
 * autres), puis, à la fin, les dernières lignes du journal quand le serveur n'a pas démarré (BR-MOD-027) et la sauvegarde
 * faite. L'agent juge tout : rien n'est supposé réussi avant qu'il le dise.
 */
export function useModsApply(
  serverId: () => string,
  service: () => ServiceInfo,
  /** L'opération finit : la liste des mods se relit. */
  onFinished: () => void,
) {
  const action = useServerAction(serverId);

  // ── L'estimation ─────────────────────────────────────────────────────────────────────────────
  const preview = ref<PreviewState>({ kind: "idle" });
  let previewing = 0;

  async function loadPreview(): Promise<void> {
    const mine = ++previewing;
    preview.value = { kind: "loading" };
    try {
      const outcome = await getLinkBridge().readBackupPreview(serverId(), service().key);
      if (mine !== previewing) return;
      preview.value =
        outcome.kind === "ready" ? { kind: "ready", preview: outcome.preview } : { kind: "failed" };
    } catch (error) {
      if (mine !== previewing) return;
      if (!failureOf(error)) logUiError(error, "modsApply:preview");
      preview.value = { kind: "failed" };
    }
  }

  // ── L'application ────────────────────────────────────────────────────────────────────────────
  const launch = ref<Launch | null>(null);
  const follow = useOperationFollow(serverId, () => launch.value?.operationId ?? null);
  const unknownResult = ref(false);
  const refusal = ref<string | undefined>();

  /** Applique le lot (appelé par la confirmation, avec le mot de passe saisi) : jamais rejoué. */
  async function perform(
    batch: ModsBatch,
    rows: ChangeRow[],
    adminPassword: string | null,
  ): Promise<ActReport> {
    refusal.value = undefined;
    const id = serverId();
    const current = service();
    const outcome = await action.run(
      () => getLinkBridge().applyMods(id, current.key, batch, adminPassword),
      {
        unknownMessage: "mods.apply.unknown",
        forbiddenMessage: "mods.apply.refused.other",
        passwordMissingIsRefusal: true,
      },
    );
    if (!outcome) {
      if (action.passwordMissing.value) {
        return { kind: "refused", refusal: { kind: "password_required" } };
      }
      return action.failedReport();
    }
    if (outcome.kind === "unknown") {
      unknownResult.value = true;
      return { kind: "unknown" };
    }
    if (outcome.kind === "accepted") {
      launch.value = {
        operationId: outcome.operationId,
        backupId: outcome.backupId,
        rows,
        count: rows.length,
        running: current.state === "running" || current.state === "restart_loop",
        name: current.name,
      };
      follow.begin(outcome.operationId);
      return { kind: "done" };
    }
    const reason = outcome.refusal;
    if (reason.kind === "common") {
      const inner = reason.refusal;
      if (
        inner.kind === "wrong_password" ||
        inner.kind === "password_required" ||
        inner.kind === "too_many_attempts" ||
        inner.kind === "busy"
      ) {
        return { kind: "refused", refusal: inner };
      }
    }
    refusal.value = applyRefusalText(reason);
    // L'espace a changé depuis l'estimation : l'écran relit ses chiffres sans perdre le lot.
    if (reason.kind === "common" && reason.refusal.kind === "no_space") void loadPreview();
    return { kind: "refused", refusal: { kind: "other" } };
  }

  // ── L'issue ──────────────────────────────────────────────────────────────────────────────────
  const outcome = computed(() => {
    const done = follow.finished.value;
    return done ? outcomeOf(done.operation.status) : null;
  });
  const tail = ref<OperationTail | null>(null);
  const backup = ref<BackupInfo | null>(null);

  watch(follow.finished, async (done) => {
    const current = launch.value;
    if (!done || !current) return;
    onFinished();
    const kind = outcomeOf(done.operation.status);
    // Les dernières lignes du journal : seulement quand le serveur n'a pas démarré avec ces mods.
    if (kind === "rolled_back" || kind === "failed") {
      try {
        const read = await getLinkBridge().readOperationTail(serverId(), current.operationId);
        if (read.kind === "ready" && launch.value === current) tail.value = read.tail;
      } catch (error) {
        if (!failureOf(error)) logUiError(error, "modsApply:tail");
      }
    }
    // La sauvegarde faite avant : sa note la nomme.
    try {
      const read = await getLinkBridge().readBackups(serverId(), service().key);
      if (read.kind === "ready" && launch.value === current) {
        backup.value = read.view.backups.find((item) => item.id === current.backupId) ?? null;
      }
    } catch (error) {
      if (!failureOf(error)) logUiError(error, "modsApply:backup");
    }
  });

  /** « Annuler l'opération » : possible jusqu'au début de la mise en place (BR-MOD-026). */
  async function cancel(adminPassword: string | null): Promise<ActReport> {
    const current = launch.value;
    if (!current) return { kind: "done" };
    const outcome = await action.run(
      () => getLinkBridge().cancelServiceOperation(serverId(), current.operationId, adminPassword),
      { unknownMessage: "mods.apply.unknown", passwordMissingIsRefusal: true },
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

  /** L'issue a été dite : acquittée, jamais rejouée. */
  function dismiss(): void {
    follow.settle();
    launch.value = null;
    tail.value = null;
    backup.value = null;
    unknownResult.value = false;
  }

  /** L'écran quitte l'opération sans la fermer : son issue sera annoncée comme les autres. */
  function leave(): void {
    follow.release();
  }

  return {
    preview,
    loadPreview,
    launch,
    operation: follow.operation,
    finished: follow.finished,
    outcome,
    tail,
    backup,
    unknownResult,
    refusal,
    perform,
    cancel,
    dismiss,
    leave,
  };
}

export type { PendingAdd };
