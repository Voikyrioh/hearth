import { isHearthUsername } from "@/accounts/name";
import { type MessageKey, t } from "@/i18n";
import type {
  DecisionRefusal,
  OperationChoice,
  OperationKind,
  OperationStatus,
  ServiceOperation,
} from "@/link";

/**
 * Les textes des opérations de service (HRT-84) : des fonctions pures, sans E/S. Aucun texte ne vient de l'agent :
 * il envoie des CODES (genre, statut, étape, cause, choix), l'interface les met en phrases. Un code qu'elle ne
 * connaît pas ne devient jamais une phrase inventée ni un code affiché tel quel.
 */

export const LABEL_KEYS: Record<OperationKind, MessageKey> = {
  start: "serviceOps.label.start",
  stop: "serviceOps.label.stop",
  restart: "serviceOps.label.restart",
  deploy: "serviceOps.label.deploy",
  update: "serviceOps.label.update",
  delete: "serviceOps.label.delete",
  resources: "serviceOps.label.resources",
  backup: "serviceOps.label.backup",
  restore: "serviceOps.label.restore",
  world_import: "serviceOps.label.worldImport",
  unknown: "serviceOps.label.unknown",
};

const SUBJECT_KEYS: Record<OperationKind, MessageKey> = {
  start: "serviceOps.subject.start",
  stop: "serviceOps.subject.stop",
  restart: "serviceOps.subject.restart",
  deploy: "serviceOps.subject.deploy",
  update: "serviceOps.subject.update",
  delete: "serviceOps.subject.delete",
  resources: "serviceOps.subject.resources",
  backup: "serviceOps.subject.backup",
  restore: "serviceOps.subject.restore",
  world_import: "serviceOps.subject.worldImport",
  unknown: "serviceOps.subject.unknown",
};

const STEP_KEYS: Record<string, MessageKey> = {
  check: "serviceOps.steps.check",
  pull: "serviceOps.steps.pull",
  stop: "serviceOps.steps.stop",
  stop_old: "serviceOps.steps.stopOld",
  recreate: "serviceOps.steps.recreate",
  create_new: "serviceOps.steps.createNew",
  start: "serviceOps.steps.start",
  verify: "serviceOps.steps.verify",
  rollback: "serviceOps.steps.rollback",
  copy: "serviceOps.steps.copy",
  erase: "serviceOps.steps.erase",
};

const STATUS_KEYS: Record<OperationStatus, MessageKey> = {
  accepted: "serviceOps.status.accepted",
  running: "serviceOps.status.running",
  awaiting_decision: "serviceOps.status.awaitingDecision",
  needs_attention: "serviceOps.status.needsAttention",
  cancelling: "serviceOps.status.cancelling",
  rolling_back: "serviceOps.status.rollingBack",
  cancelled: "serviceOps.status.running",
  rolled_back: "serviceOps.status.running",
  succeeded: "serviceOps.status.running",
  failed: "serviceOps.status.running",
  interrupted: "serviceOps.status.running",
  unknown: "serviceOps.status.unknown",
};

const OUTCOME_KEYS: Partial<Record<OperationStatus, MessageKey>> = {
  succeeded: "serviceOps.outcome.succeeded",
  failed: "serviceOps.outcome.failed",
  cancelled: "serviceOps.outcome.cancelled",
  rolled_back: "serviceOps.outcome.rolledBack",
  interrupted: "serviceOps.outcome.interrupted",
};

export const CHOICE_KEYS: Record<OperationChoice, MessageKey> = {
  continue: "serviceOps.choice.continue",
  cancel: "serviceOps.choice.cancel",
  revert: "serviceOps.choice.revert",
  keep: "serviceOps.choice.keep",
  verified: "serviceOps.choice.verified",
};

const REFUSAL_KEYS: Partial<Record<DecisionRefusal["kind"], MessageKey>> = {
  not_found: "serviceOps.refused.notFound",
  not_waiting: "serviceOps.refused.notWaiting",
  not_offered: "serviceOps.refused.notOffered",
  not_applicable: "serviceOps.refused.notApplicable",
  expired: "serviceOps.refused.expired",
  too_late: "serviceOps.refused.tooLate",
  changed: "serviceOps.refused.changed",
};

/** Le texte d'un refus de décision qui n'est pas celui de la confirmation (le mot de passe se dit sous son champ). */
export function refusalText(refusal: DecisionRefusal): string {
  const key = REFUSAL_KEYS[refusal.kind] ?? "serviceOps.refused.other";
  return t(key);
}

/**
 * Le titre d'une opération. L'effacement des données d'un service est dit sans chemin ni progression à un compte
 * qui n'a pas le droit de gérer les services (BR-SVC-029) : `detailed` faux.
 */
export function operationTitle(
  operation: ServiceOperation,
  name: string,
  detailed: boolean,
): string {
  if (operation.kind === "delete") {
    return t(detailed ? "serviceOps.eraseWithRight" : "serviceOps.eraseNoRight", { name });
  }
  return t(LABEL_KEYS[operation.kind], { name });
}

/** Le texte de l'étape en cours, ou `null` (étape absente ou inconnue : jamais le code lui-même). */
export function stepText(operation: ServiceOperation): string | null {
  if (operation.step === null) return null;
  const key = STEP_KEYS[operation.step];
  return key ? t(key) : null;
}

/** Une ligne d'état de l'opération (en attente, en cours, réponse attendue…). */
export function statusText(operation: ServiceOperation): string {
  return t(STATUS_KEYS[operation.status]);
}

/** La phrase d'issue d'une opération terminée, ou `null` si elle ne l'est pas. */
export function outcomeText(operation: ServiceOperation, name: string): string | null {
  const key = OUTCOME_KEYS[operation.status];
  if (!key) return null;
  return t(key, { subject: t(SUBJECT_KEYS[operation.kind], { name }) });
}

/** Une taille en octets, en unités binaires lisibles (Ko, Mo, Go) ; arrondie, jamais inventée. */
export function formatSize(bytes: number): string {
  const units = ["o", "Ko", "Mo", "Go", "To"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = unit === 0 || value >= 100 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${String(rounded).replace(".", ",")} ${units[unit]}`;
}

/** La progression à montrer : un ratio pour la jauge (`null` : aucune jauge) et son texte. */
export function progressOf(
  operation: ServiceOperation,
  detailed: boolean,
): { ratio: number | null; text: string | null } {
  if (operation.kind === "delete" && !detailed) return { ratio: null, text: null };
  const ratio = operation.percent === null ? null : operation.percent / 100;
  const parts: string[] = [];
  if (operation.percent !== null) parts.push(t("serviceOps.percent", { n: operation.percent }));
  if (operation.bytesDone !== null && operation.bytesTotal !== null) {
    parts.push(
      t("serviceOps.bytes", {
        done: formatSize(operation.bytesDone),
        total: formatSize(operation.bytesTotal),
      }),
    );
  } else if (operation.itemsDone !== null) {
    parts.push(
      t(operation.kind === "delete" ? "serviceOps.itemsErased" : "serviceOps.items", {
        n: operation.itemsDone,
      }),
    );
  }
  return { ratio, text: parts.length > 0 ? parts.join(" · ") : null };
}

/** « Lancée par {compte}. » : seulement pour un identifiant de compte valide (texte de l'agent), sinon `null`. */
export function launchedBy(account: string | null | undefined): string | null {
  if (!account || !isHearthUsername(account)) return null;
  return t("serviceOps.by", { account });
}
