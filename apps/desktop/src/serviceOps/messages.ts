import { isHearthUsername } from "@/accounts/name";
import { deployOutcome } from "@/deploy/messages";
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
  create: "serviceOps.steps.create",
  watch: "serviceOps.steps.watch",
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

/** Le texte d'un statut EN COURS ; un statut définitif n'en a pas (son issue se dit par `outcomeText`). */
const STATUS_KEYS: Partial<Record<OperationStatus, MessageKey>> = {
  accepted: "serviceOps.status.accepted",
  running: "serviceOps.status.running",
  awaiting_decision: "serviceOps.status.awaitingDecision",
  needs_attention: "serviceOps.status.needsAttention",
  cancelling: "serviceOps.status.cancelling",
  rolling_back: "serviceOps.status.rollingBack",
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

/**
 * Le texte d'un refus de décision qui n'est pas celui de la confirmation (le mot de passe se dit sous son champ).
 * Un code que ce client ne connaît pas devient le refus générique.
 */
export function refusalText(refusal: { kind: string }): string {
  const key = REFUSAL_KEYS[refusal.kind as DecisionRefusal["kind"]] ?? "serviceOps.refused.other";
  return t(key);
}

/**
 * Le titre d'une opération. L'effacement des données d'un service est dit sans chemin ni progression à un compte
 * qui n'a pas le droit de gérer les services (BR-SVC-029, BR-DEL-031) : `detailed` faux.
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

/** Une ligne d'état de l'opération (en attente, en cours, réponse attendue…), `null` pour un statut définitif. */
export function statusText(operation: ServiceOperation): string | null {
  const key = STATUS_KEYS[operation.status];
  return key ? t(key) : null;
}

/** La phrase d'issue d'une opération terminée, ou `null` si elle ne l'est pas. */
export function outcomeText(operation: ServiceOperation, name: string): string | null {
  return outcomeOf(operation, name)?.message ?? null;
}

/** Le ton d'une notification d'issue. */
export type OutcomeTone = "success" | "error" | "warn" | "info";

const LIFECYCLE_KINDS: readonly OperationKind[] = ["start", "stop", "restart"];

/**
 * L'issue d'une opération de pilotage (démarrer, arrêter, redémarrer), dite par sa CAUSE (HRT-65, BR-LIFE-004, 005,
 * 009, 011, 012, 019, 021) : jamais un texte de l'agent. `null` : pas une opération de pilotage, ou un statut qui
 * n'a pas fini ; l'appelant dit alors l'issue générale.
 */
function lifecycleOutcome(
  operation: ServiceOperation,
  name: string,
): { message: string; tone: OutcomeTone } | null {
  if (!LIFECYCLE_KINDS.includes(operation.kind)) return null;
  const say = (key: MessageKey, tone: OutcomeTone) => ({ message: t(key, { name }), tone });
  const cause = operation.cause;
  switch (operation.status) {
    case "succeeded":
      if (cause === "forced_stop") return say("lifecycle.outcome.forcedStop", "warn");
      if (cause === "restarted_by_host") return say("lifecycle.outcome.restartedByHost", "warn");
      if (cause === "stopped_right_after_start") {
        return say("lifecycle.outcome.stoppedRightAfterStart", "error");
      }
      if (operation.kind === "start") return say("lifecycle.outcome.started", "success");
      if (operation.kind === "stop") return say("lifecycle.outcome.stopped", "success");
      return say("lifecycle.outcome.restarted", "success");
    case "failed":
      switch (cause) {
        case "port_in_use":
          return say("lifecycle.outcome.portInUse", "error");
        case "engine_refused":
          return say("lifecycle.outcome.engineRefused", "error");
        case "engine_unavailable":
          return say("lifecycle.outcome.engineUnavailable", "error");
        case "service_not_found":
          return say("lifecycle.outcome.serviceNotFound", "error");
        case "step_failed":
          return operation.kind === "restart"
            ? say("lifecycle.outcome.restartFailed", "error")
            : null;
        default:
          return null;
      }
    case "interrupted":
      if (cause === "timeout") return say("lifecycle.outcome.timeout", "warn");
      if (cause === "engine_unavailable") return say("lifecycle.outcome.engineLost", "warn");
      if (cause === "interrupted") return say("lifecycle.outcome.agentRestarted", "warn");
      return null;
    default:
      return null;
  }
}

/** L'issue d'une opération terminée : sa phrase et son ton, ou `null` si elle ne l'est pas. */
export function outcomeOf(
  operation: ServiceOperation,
  name: string,
): { message: string; tone: OutcomeTone } | null {
  const key = OUTCOME_KEYS[operation.status];
  if (!key) return null;
  // Un déploiement (HRT-69) se dit par sa cause : le service est en marche, ou pourquoi rien n'a été créé.
  const specific = deployOutcome(operation, name) ?? lifecycleOutcome(operation, name);
  if (specific) return specific;
  const tone: OutcomeTone =
    operation.status === "succeeded" ? "success" : operation.status === "failed" ? "error" : "warn";
  return { message: t(key, { subject: t(SUBJECT_KEYS[operation.kind], { name }) }), tone };
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
export function launchedBy(
  account: string | null | undefined,
  self?: string | null,
): string | null {
  if (!account || !isHearthUsername(account)) return null;
  // FIX:01M5A5VH5GR2M9PF9Y1SB04D65 : l'action lancée par le compte connecté se dit « par vous ».
  if (self && account === self) return t("serviceOps.byYou");
  return t("serviceOps.by", { account });
}
