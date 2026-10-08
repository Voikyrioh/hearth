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

/** Une étape finie n'a plus de points de suspension : elle est faite, plus en train de se faire. */
const STEP_DONE: Record<Exclude<UpdateStep, "done">, MessageKey> = {
  download: "agentUpdate.stepDownloadDone",
  verify: "agentUpdate.stepVerifyDone",
  install: "agentUpdate.stepInstallDone",
  restart: "agentUpdate.stepRestartDone",
  check: "agentUpdate.stepCheckDone",
};

/** Le libellé d'une étape dans la liste (« Téléchargement : 35 % » pendant le téléchargement). */
export function stepLabel(
  step: Exclude<UpdateStep, "done">,
  percent: number | null,
  done = false,
): string {
  if (done) return t(STEP_DONE[step]);
  if (step === "download" && percent !== null) {
    return t("agentUpdate.stepDownloadPercent", { percent });
  }
  return t(STEP_LABELS[step]);
}

/** La phrase d'avancement : l'étape n'y est PAS redite, la liste des étapes la montre (HRT-46, C43, FIX:01M4E48N732694TTFSRQF1C4KG). */
export function progressSentence(): string {
  return t("agentUpdate.progress");
}

/**
 * Ce que le lecteur d'écran entend (région vivante polie, invisible) : l'étape COURANTE, une fois par étape (pas à chaque
 * pourcentage). À l'œil, l'étape n'est dite que par la liste (HRT-46, C43).
 */
export function stepAnnouncement(step: Exclude<UpdateStep, "done">): string {
  return t("agentUpdate.announce", { step: stepLabel(step, null, true).toLowerCase() });
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

const REFUSALS: Record<Exclude<AgentUpdateRefusal["kind"], "too_many_attempts">, MessageKey> = {
  managed_install: "agentUpdate.managed",
  in_progress: "agentUpdate.refusedInProgress",
  bad_signature: "agentUpdate.refusedBadSignature",
  invalid_target: "agentUpdate.refusedInvalidTarget",
  no_target: "agentUpdate.refusedNoTarget",
  target_changed: "agentUpdate.refusedTargetChanged",
  not_newer: "agentUpdate.refusedNotNewer",
  wrong_password: "reauth.wrongPassword",
  password_required: "reauth.elapsed",
  busy: "reauth.busy",
  other: "agentUpdate.refusedOther",
};

/** Le message d'un refus de la demande (jamais le texte de l'agent). */
export function refusalMessage(refusal: AgentUpdateRefusal): string {
  if (refusal.kind === "too_many_attempts") {
    return t("reauth.waiting", { n: refusal.retry_after_s });
  }
  return t(REFUSALS[refusal.kind]);
}
