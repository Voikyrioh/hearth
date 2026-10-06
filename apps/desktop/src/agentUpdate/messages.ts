import { type MessageKey, t } from "@/i18n";
import type { AgentUpdateRefusal, UpdateReason, UpdateStep } from "@/link";

/**
 * Les textes de la mise à jour de l'agent, indexés par les CODES de l'agent (étape, issue, raison,
 * refus) : l'interface ne montre jamais un message de l'agent, il en est le diagnostic seulement.
 * Les clés sont des correspondances EXHAUSTIVES : un code de plus ne compile pas sans son texte.
 */

const STEP_LABELS: Record<Exclude<UpdateStep, "done">, MessageKey> = {
  download: "agentUpdate.stepDownload",
  verify: "agentUpdate.stepVerify",
  install: "agentUpdate.stepInstall",
  restart: "agentUpdate.stepRestart",
  check: "agentUpdate.stepCheck",
};

const STEP_NOW: Record<Exclude<UpdateStep, "done" | "download">, MessageKey> = {
  verify: "agentUpdate.nowVerify",
  install: "agentUpdate.nowInstall",
  restart: "agentUpdate.nowRestart",
  check: "agentUpdate.nowCheck",
};

/** Le libellé d'une étape dans la liste (« Téléchargement : 35 % » pendant le téléchargement). */
export function stepLabel(step: Exclude<UpdateStep, "done">, percent: number | null): string {
  if (step === "download" && percent !== null) {
    return t("agentUpdate.stepDownloadPercent", { percent });
  }
  return t(STEP_LABELS[step]);
}

/** La phrase d'avancement : « Mise à jour de l'agent en cours. Étape : téléchargement (35 %)… ». */
export function progressSentence(
  step: Exclude<UpdateStep, "done">,
  percent: number | null,
): string {
  const now =
    step === "download"
      ? percent === null
        ? t("agentUpdate.nowDownload")
        : t("agentUpdate.nowDownloadPercent", { percent })
      : t(STEP_NOW[step]);
  return t("agentUpdate.progress", { step: now });
}

const REASONS: Record<UpdateReason, MessageKey> = {
  unreachable: "agentUpdate.resultUnreachable",
  download_failed: "agentUpdate.resultDownloadFailed",
  bad_checksum: "agentUpdate.resultBadChecksum",
  bad_signature: "agentUpdate.resultBadSignature",
  bad_binary: "agentUpdate.resultBadBinary",
  staging: "agentUpdate.resultNotInstalled",
  swap: "agentUpdate.resultNotInstalled",
  supervisor_launch: "agentUpdate.resultNotInstalled",
  no_answer: "agentUpdate.resultNoAnswer",
  identity_changed: "agentUpdate.resultIdentityChanged",
  interrupted: "agentUpdate.resultInterrupted",
  rollback_failed: "agentUpdate.resultRollbackFailed",
  unknown: "agentUpdate.resultFailed",
};

/** Un résultat « bon », « annulé » ou « échoué » : le ton du message (et de son encadré). */
export type ResultTone = "ok" | "warn" | "crit";

export interface ResultLike {
  outcome: "succeeded" | "rolled_back" | "failed";
  reason: UpdateReason | null;
  /** `null` ou vide : la version visée ne se sait pas, le texte n'a pas de numéro. */
  version: string | null;
}

/** Le message d'un résultat de mise à jour (BR-UPDATE-013, 015, 017, 019). */
export function resultMessage(result: ResultLike): string {
  if (result.outcome === "succeeded") {
    return result.version
      ? t("agentUpdate.resultSucceeded", { version: result.version })
      : t("agentUpdate.resultSucceededNoVersion");
  }
  if (result.outcome === "rolled_back") {
    // Seules deux raisons existent pour un retour arrière ; toute autre se dit sans la raison.
    return result.reason === "no_answer"
      ? t("agentUpdate.resultNoAnswer")
      : result.reason === "identity_changed"
        ? t("agentUpdate.resultIdentityChanged")
        : t("agentUpdate.resultRolledBack");
  }
  return t(REASONS[result.reason ?? "unknown"]);
}

export function resultTone(result: ResultLike): ResultTone {
  if (result.outcome === "succeeded") return "ok";
  return result.outcome === "rolled_back" ? "warn" : "crit";
}

/** La ligne d'historique d'un résultat qui n'est plus récent (plus de 24 h). */
export function historyMessage(result: ResultLike, when: string): string {
  const key: MessageKey =
    result.outcome === "succeeded"
      ? "agentUpdate.historySucceeded"
      : result.outcome === "rolled_back"
        ? "agentUpdate.historyRolledBack"
        : "agentUpdate.historyFailed";
  return t(key, { when });
}

const REFUSALS: Record<AgentUpdateRefusal["kind"], MessageKey> = {
  managed_install: "agentUpdate.managed",
  in_progress: "agentUpdate.refusedInProgress",
  bad_signature: "agentUpdate.refusedBadSignature",
  invalid_target: "agentUpdate.refusedInvalidTarget",
  no_target: "agentUpdate.refusedNoTarget",
  target_changed: "agentUpdate.refusedTargetChanged",
  not_newer: "agentUpdate.refusedNotNewer",
  other: "agentUpdate.refusedOther",
};

/** Le message d'un refus de la demande (jamais le texte de l'agent). */
export function refusalMessage(refusal: AgentUpdateRefusal): string {
  return t(REFUSALS[refusal.kind]);
}
