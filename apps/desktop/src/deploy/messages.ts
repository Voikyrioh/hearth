import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type {
  CancelRefusal,
  DeployCheck,
  DeployField,
  DeployRefusal,
  EngineState,
  PortCheck,
  ServiceOperation,
} from "@/link";
import { exitCodeText } from "@/services/detail";
import { ENGINE_STATE_KEYS } from "@/services/format";
import type {
  CommandIssue,
  EnvNameIssue,
  EnvValueIssue,
  ImageIssue,
  NameIssue,
  PortIssue,
  TagIssue,
} from "./rules";

/**
 * Les textes d'un déploiement (HRT-69) : des fonctions pures, sans E/S. Aucun texte ne vient de l'agent : il envoie des
 * CODES (constats, refus, causes), l'interface les met en phrases. Un texte du serveur qui doit entrer dans une phrase
 * (le nom d'un service, d'un programme) est rendu inoffensif (`safeText`) : jamais du HTML.
 */

const NAME_KEYS: Record<NameIssue, MessageKey> = {
  required: "deploy.nameRequired",
  too_short: "deploy.nameTooShort",
  too_long: "deploy.nameTooLong",
  first: "deploy.nameFirst",
  chars: "deploy.nameChars",
};
export const nameText = (issue: NameIssue): string => t(NAME_KEYS[issue]);

const IMAGE_KEYS: Record<ImageIssue, MessageKey> = {
  required: "deploy.imageRequired",
  digest: "deploy.imageDigest",
  too_long: "deploy.imageTooLong",
  format: "deploy.imageFormat",
};
export const imageText = (issue: ImageIssue): string => t(IMAGE_KEYS[issue]);

const TAG_KEYS: Record<TagIssue, MessageKey> = {
  format: "deploy.tagFormat",
  too_long: "deploy.tagFormat",
  conflict: "deploy.tagConflict",
};
export const tagText = (issue: TagIssue): string => t(TAG_KEYS[issue]);

const ENV_NAME_KEYS: Record<EnvNameIssue, MessageKey> = {
  required: "deploy.envNameRequired",
  format: "deploy.envNameFormat",
  duplicate: "deploy.envNameDuplicate",
  too_long: "deploy.envNameTooLong",
};
export const envNameText = (issue: EnvNameIssue): string => t(ENV_NAME_KEYS[issue]);

const ENV_VALUE_KEYS: Record<EnvValueIssue, MessageKey> = {
  too_long: "deploy.envValueTooLong",
  multiline: "deploy.envValueMultiline",
};
export const envValueText = (issue: EnvValueIssue): string => t(ENV_VALUE_KEYS[issue]);

const PORT_KEYS: Record<PortIssue, MessageKey> = {
  required: "deploy.portRequired",
  integer: "deploy.portInteger",
  range: "deploy.portRange",
};
/** Le défaut d'un port du serveur ; pour un port du service, « requis » dit « Le port du service est requis ». */
export function portText(issue: PortIssue, which: "host" | "container"): string {
  if (issue === "required" && which === "container") return t("deploy.containerPortRequired");
  return t(PORT_KEYS[issue]);
}

const COMMAND_KEYS: Record<CommandIssue, MessageKey> = {
  too_long: "deploy.commandTooLong",
  quote: "deploy.commandQuote",
  control: "deploy.commandControl",
  args: "deploy.commandArgs",
};
export const commandText = (issue: CommandIssue): string => t(COMMAND_KEYS[issue]);

// ── Le constat du serveur sur un port (BR-DEPLOY-008, 009) ──────────────────────────────────────

export interface PortVerdict {
  /** Bloque le lancement. */
  error: string | null;
  /** N'empêche rien (port bas, port réservé par un service arrêté). */
  warning: string | null;
  /** Le port réservé par un service arrêté demande une case dans le récapitulatif (BR-DEPLOY-008). */
  reserved: boolean;
  suggested: number | null;
}

const NONE: PortVerdict = { error: null, warning: null, reserved: false, suggested: null };

/** Ce que le serveur a dit d'un port : un texte d'erreur, d'avertissement, la proposition de port libre. */
export function portVerdict(check: PortCheck | undefined): PortVerdict {
  if (!check) return NONE;
  const holder = check.holder === null ? null : safeText(check.holder);
  const privileged = check.privileged ? t("deploy.portPrivileged") : null;
  switch (check.status) {
    case "held_by_service":
      return {
        ...NONE,
        error:
          holder === null ? t("deploy.portProgram") : t("deploy.portService", { name: holder }),
        suggested: check.suggested,
      };
    case "held_by_unmanaged":
      return { ...NONE, error: t("deploy.portUnmanaged"), suggested: check.suggested };
    case "held_by_program":
      return {
        ...NONE,
        error:
          holder === null
            ? t("deploy.portProgram")
            : t("deploy.portProgramBy", { program: holder }),
        suggested: check.suggested,
      };
    case "agent_port":
      return { ...NONE, error: t("deploy.portAgent"), suggested: check.suggested };
    case "reserved_by_stopped":
      return {
        ...NONE,
        warning:
          holder === null
            ? t("deploy.portReservedAnon")
            : t("deploy.portReserved", { name: holder }),
        reserved: true,
      };
    default:
      return { ...NONE, warning: privileged };
  }
}

// ── Les refus de l'agent (BR-DEPLOY-030, 032, 035) ──────────────────────────────────────────────

const FIELD_KEYS: Record<DeployField, MessageKey> = {
  name: "deploy.fieldName",
  image: "deploy.fieldImage",
  tag: "deploy.fieldTag",
  env: "deploy.fieldEnv",
  ports: "deploy.fieldPorts",
  mounts: "deploy.fieldMounts",
  command: "deploy.fieldCommand",
  resources: "deploy.fieldResources",
};

/** L'occupant d'un port, pour « Le port {port} vient d'être pris par {occupant} » : jamais le nom d'un conteneur hors Hearth. */
export function occupantText(status: PortCheck["status"], holder: string | null): string {
  if (status === "held_by_unmanaged") return t("deploy.occupantUnmanaged");
  if (holder !== null && status === "held_by_service") {
    return t("deploy.occupantService", { name: safeText(holder) });
  }
  if (status === "held_by_program" || status === "agent_port") return t("deploy.occupantProgram");
  return t("deploy.occupantUnknown");
}

/**
 * Ce qui empêche de lancer, d'après un constat du serveur : un nom pris ou en cours d'effacement, un port tenu. UNE
 * seule règle (BR-DEPLOY-008) : le pied du formulaire, la revérification au clic et le récapitulatif la partagent.
 * `null` : rien ne bloque (un port réservé par un service arrêté se coche dans le récapitulatif).
 */
export function checkBlocker(check: DeployCheck): { text: string; field: DeployField } | null {
  if (check.name === "taken") return { text: t("deploy.nameTaken"), field: "name" };
  if (check.name === "being_deleted") {
    return { text: t("deploy.nameBeingDeleted"), field: "name" };
  }
  for (const port of check.ports) {
    if (portVerdict(port).error === null) continue;
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

/** Le champ du formulaire qu'un refus désigne, `null` : un refus qui ne vise aucun champ. */
export function refusalField(refusal: DeployRefusal): DeployField | null {
  switch (refusal.kind) {
    case "name_taken":
    case "name_being_deleted":
      return "name";
    case "port_in_use":
      return "ports";
    case "invalid":
      return refusal.field;
    case "limits_required":
      return "resources";
    default:
      return null;
  }
}

/** Les refus qui ramènent au champ signalé (la fenêtre se ferme, la saisie reste) : BR-DEPLOY-032. */
export function isFieldRefusal(refusal: DeployRefusal): boolean {
  return refusalField(refusal) !== null;
}

/** La phrase d'un refus de l'agent (rien n'a été créé). */
export function refusalText(
  refused: DeployRefusal | { kind: string },
  engineState?: EngineState | null,
): string {
  // Un code que ce client ne connaît pas tombe dans `default` : le refus générique, jamais une phrase inventée.
  const refusal = refused as DeployRefusal;
  switch (refusal.kind) {
    case "name_taken":
      return t("deploy.nameTaken");
    case "name_being_deleted":
      return t("deploy.nameBeingDeleted");
    case "port_in_use":
      return t("deploy.refusedPort", {
        port: refusal.port,
        occupant: occupantText(refusal.status, refusal.holder),
      });
    case "invalid":
      return t("deploy.refusedField", { field: t(FIELD_KEYS[refusal.field]) });
    case "limits_required":
      return t("deploy.refusedLimits");
    case "setting_refused":
      return t("deploy.refusedSetting");
    case "deploy_limit":
      return t("deploy.refusedLimit");
    case "engine_unavailable": {
      const state = refusal.engine ?? engineState ?? null;
      const key = state && state !== "checking" ? ENGINE_STATE_KEYS[state] : undefined;
      return key ? t("deploy.refusedEngine", { state: t(key) }) : t("deploy.refusedEngineBare");
    }
    case "busy":
      return t("deploy.refusedBusy");
    case "no_space":
      return t("deploy.lowDiskUnknown");
    default:
      return t("deploy.refusedOther");
  }
}

const CANCEL_KEYS: Partial<Record<CancelRefusal["kind"], MessageKey>> = {
  not_found: "deploy.cancelNotFound",
  too_late: "deploy.tooLate",
  not_cancellable: "deploy.cancelNotCancellable",
  changed: "deploy.cancelChanged",
};

/** Le texte d'un refus d'annulation qui n'est pas celui de la confirmation (le mot de passe se dit sous son champ). */
export function cancelRefusalText(refusal: { kind: string }): string {
  return t(CANCEL_KEYS[refusal.kind as CancelRefusal["kind"]] ?? "deploy.cancelOther");
}

// ── L'issue d'un déploiement, dite par sa CAUSE (BR-DEPLOY-017, 019, 020, 034) ───────────────────

interface Cause {
  long: MessageKey;
  short: MessageKey;
  /** Le champ du formulaire à montrer quand on le rouvre. */
  field: DeployField | null;
}

const CAUSES: Record<string, Cause> = {
  image_not_found: {
    long: "deploy.failImageNotFound",
    short: "deploy.short.imageNotFound",
    field: "image",
  },
  tag_not_found: {
    long: "deploy.failTagNotFound",
    short: "deploy.short.tagNotFound",
    field: "tag",
  },
  image_private_or_missing: {
    long: "deploy.failPrivate",
    short: "deploy.short.private",
    field: "image",
  },
  registry_unreachable: {
    long: "deploy.failRegistry",
    short: "deploy.short.registry",
    field: null,
  },
  registry_rate_limited: {
    long: "deploy.failRateLimited",
    short: "deploy.short.rateLimited",
    field: null,
  },
  platform_mismatch: {
    long: "deploy.failPlatform",
    short: "deploy.short.platform",
    field: "image",
  },
  disk_full: { long: "deploy.failDisk", short: "deploy.short.disk", field: null },
  name_taken: { long: "deploy.failName", short: "deploy.short.name", field: "name" },
  port_in_use: { long: "deploy.failPort", short: "deploy.short.port", field: "ports" },
  engine_refused: {
    long: "deploy.failEngineRefused",
    short: "deploy.short.engineRefused",
    field: null,
  },
  engine_unavailable: {
    long: "deploy.failEngineUnavailable",
    short: "deploy.short.engineUnavailable",
    field: null,
  },
  timeout: { long: "deploy.failTimeout", short: "deploy.short.timeout", field: null },
};

const OTHER: Cause = { long: "deploy.failOther", short: "deploy.short.other", field: null };

function causeOf(operation: ServiceOperation): Cause {
  return (operation.cause !== null ? CAUSES[operation.cause] : undefined) ?? OTHER;
}

/** La phrase complète d'un échec où rien n'a été créé. */
export function failureText(operation: ServiceOperation): string {
  return t(causeOf(operation).long);
}

/** Le champ à signaler quand le formulaire se rouvre après cet échec. */
export function failureField(operation: ServiceOperation): DeployField | null {
  return causeOf(operation).field;
}

/** « code 137 : coupé de force… » ou « code 1 », en minuscules pour entrer dans une phrase. */
export function exitSummary(code: number | null): string {
  if (code === null) return "";
  const text = exitCodeText(code);
  return text.charAt(0).toLowerCase() + text.slice(1);
}

/**
 * L'issue d'un déploiement annoncée hors de la fenêtre de progression (toast), ou `null` : ce n'est pas un déploiement
 * ou son statut n'a pas fini. « Le service {nom} est en marche. » ; « Le déploiement de {nom} a échoué : {cause}. ».
 */
export function deployOutcome(
  operation: ServiceOperation,
  name: string,
): { message: string; tone: "success" | "error" | "warn" | "info" } | null {
  if (operation.kind !== "deploy") return null;
  const shown = safeText(name);
  switch (operation.status) {
    case "succeeded":
      return operation.cause === "stopped_right_after_start"
        ? { message: t("lifecycle.outcome.stoppedRightAfterStart", { name: shown }), tone: "error" }
        : { message: t("deploy.success", { name: shown }), tone: "success" };
    case "failed":
      return {
        message: t("deploy.failedBackground", {
          name: shown,
          cause: t(causeOf(operation).short),
        }),
        tone: "error",
      };
    case "cancelled":
      return { message: t("deploy.cancelled"), tone: "info" };
    case "interrupted":
      return { message: t("deploy.interrupted", { name: shown }), tone: "warn" };
    default:
      return null;
  }
}
