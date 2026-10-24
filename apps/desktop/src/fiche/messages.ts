import { safeText } from "@/audit/text";
import type { FormRefusal } from "@/composables/useDeployFlow";
import { type MessageKey, t } from "@/i18n";
import type {
  DeployRefusal,
  FicheDetail,
  FicheDownload,
  FicheQuestion,
  FicheStop,
  QuestionKind,
} from "@/link";
import { formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";
import type { AnswerIssue } from "./answers";
import type { AttentionPoint } from "./attention";
import { characterName } from "./character";
import type { Coverage, DiskVerdict, Lack, MemoryNotice, MinimumVerdict } from "./fit";
import { type ProofKind, proofOf } from "./proof";

/**
 * Les textes de l'installation depuis une fiche (HRT-102), des fonctions pures : les CODES des règles
 * (`answers.ts`, `fit.ts`, `attention.ts`) et de l'agent mis en phrases, vouvoiement et impératif, jamais un texte
 * de l'agent, jamais la valeur d'une réponse (sauf UN caractère refusé). Les libellés, bornes et choix viennent de la
 * fiche ; seules les phrases autour sont d'ici.
 */

const PROOF_KEYS: Record<ProofKind, MessageKey> = {
  measured: "fiche.proof.measured",
  measured_idle: "fiche.proof.measured_idle",
  publisher: "fiche.proof.publisher",
  unmeasured: "fiche.proof.unmeasured",
};

export function proofLabel(kind: ProofKind): string {
  return t(PROOF_KEYS[kind]);
}

const PROOF_SHORT: Record<ProofKind, MessageKey> = {
  measured: "resources.proofMeasured",
  measured_idle: "resources.proofMeasuredIdle",
  publisher: "resources.proofPublisher",
  unmeasured: "fiche.proofUnmeasuredShort",
};

/** La preuve en minuscules, pour entrer dans une phrase (« mesuré », « chiffre de l'éditeur »). */
export function proofShort(kind: ProofKind): string {
  return t(PROOF_SHORT[kind]);
}

const PORT_ROLES = {
  game: "fiche.portRole.game",
  query: "fiche.portRole.query",
  voice: "fiche.portRole.voice",
  web_admin: "fiche.portRole.web_admin",
  remote_console: "fiche.portRole.remote_console",
  other: "fiche.portRole.other",
} as const;

export function portRoleText(role: string): string {
  return t(PORT_ROLES[role as keyof typeof PORT_ROLES] ?? "fiche.portRole.other");
}

const PORT_PUBLISH = {
  always: "fiche.portPublish.always",
  optional: "fiche.portPublish.optional",
  never: "fiche.portPublish.never",
} as const;

export function portPublishText(publish: string): string {
  return t(PORT_PUBLISH[publish as keyof typeof PORT_PUBLISH] ?? "fiche.portPublish.never");
}

const FOLDER_ROLES = {
  world: "fiche.folderRole.world",
  game_files: "fiche.folderRole.game_files",
  config: "fiche.folderRole.config",
  logs: "fiche.folderRole.logs",
  mods: "fiche.folderRole.mods",
  data: "fiche.folderRole.data",
} as const;

export function folderRoleText(role: string): string {
  return t(FOLDER_ROLES[role as keyof typeof FOLDER_ROLES] ?? "fiche.folderRoleOther");
}

/**
 * Une valeur de question telle qu'on la dit : un choix par son libellé (« Maître »), un oui-non par « Oui » ou « Non »,
 * jamais par le numéro ou le mot que la fiche garde en interne (revue UX 2026-10-24, constat 4). Une valeur qui n'est
 * aucun des choix se dit telle quelle.
 */
export function valueLabel(question: FicheQuestion, value: string): string {
  if (question.kind === "bool") {
    return value === question.choices[0]?.value ? t("fiche.yes") : t("fiche.no");
  }
  const choice = question.choices.find((item) => item.value === value);
  return safeText(choice ? choice.label : value);
}

/** Le genre d'une question en mots (le champ lui-même se rend d'après le genre, pas d'après ce texte). */
export function kindText(kind: QuestionKind): string {
  return t(`fiche.kind.${kind}` as MessageKey);
}

/** Les bornes d'une question en une phrase courte, `null` quand la fiche n'en pose aucune. */
export function boundsText(question: FicheQuestion): string | null {
  if (question.kind === "int") {
    if (question.min !== null && question.max !== null) {
      return t("fiche.bounds", { min: question.min, max: question.max });
    }
    if (question.min !== null) return t("fiche.boundsMin", { min: question.min });
    if (question.max !== null) return t("fiche.boundsMax", { max: question.max });
  }
  return null;
}

/** Le jeu de caractères d'un texte en une aide courte, `null` pour une ligne libre. */
export function charsetHint(question: FicheQuestion): string | null {
  if (question.charset === "file_stem") return t("fiche.charsetFileStem");
  if (question.charset === "arg_safe") return t("fiche.charsetArgSafe");
  return null;
}

/** Le caractère refusé dit en mots, puis la correction si on en connaît une (constat 5 de la revue UX du 24/10). */
function charsetText(character: string | null, suggestion: string | null): string {
  if (character === null) return t("fiche.answerIssue.charsetBare");
  const head = t("fiche.answerIssue.charset", { name: characterName(character) });
  return suggestion === null
    ? head
    : `${head} ${t("fiche.answerIssue.charsetSuggest", { suggestion: safeText(suggestion) })}`;
}

/**
 * L'aide que le client ajoute sous le champ d'une question : le jeu de caractères ou les bornes, SEULEMENT si l'aide de
 * la fiche ne les dit pas déjà (revue UX 2026-10-24, constat 18 : « de 1 à 255 » puis « Le nombre est compris entre 1 et
 * 255 »). Une aide qui cite les deux bornes, ou qui parle de lettres et de chiffres, suffit.
 */
export function extraHint(question: FicheQuestion, ficheHelp: string): string | null {
  const charset = charsetHint(question);
  if (charset !== null) return /lettres|chiffres/i.test(ficheHelp) ? null : charset;
  const bounds = boundsText(question);
  if (bounds === null) return null;
  const quoted = [question.min, question.max]
    .filter((value): value is number => value !== null)
    .map((value) => new RegExp(`(?<!\\d)${value}(?!\\d)`));
  return quoted.length > 0 && quoted.every((pattern) => pattern.test(ficheHelp)) ? null : bounds;
}

/** La phrase d'un défaut de réponse. Le caractère refusé est cité, jamais la valeur. */
export function answerIssueText(issue: AnswerIssue): string {
  switch (issue.code) {
    case "required":
      return t("fiche.answerIssue.required");
    case "too_short":
      return t("fiche.answerIssue.tooShort", { min: issue.min });
    case "too_long":
      return t("fiche.answerIssue.tooLong", { max: issue.max });
    case "multiline":
      return t("fiche.answerIssue.multiline");
    case "charset":
      return charsetText(issue.character, issue.suggestion ?? null);
    case "not_integer":
      return t("fiche.answerIssue.notInteger");
    case "out_of_range":
      if (issue.min !== null && issue.max !== null) {
        return t("fiche.answerIssue.range", { min: issue.min, max: issue.max });
      }
      return issue.min !== null
        ? t("fiche.answerIssue.rangeMin", { min: issue.min })
        : t("fiche.answerIssue.rangeMax", { max: issue.max ?? 0 });
    case "bad_choice":
      return t("fiche.answerIssue.badChoice");
  }
}

/** Une durée en secondes dite en mots courts (« 30 secondes », « 15 minutes »). */
export function durationText(seconds: number): string {
  if (seconds < 120) return t("fiche.seconds", { n: seconds });
  const minutes = Math.round(seconds / 60);
  if (minutes < 120) return t("fiche.minutes", { n: minutes });
  return t("fiche.hours", { n: Math.round(minutes / 6) / 10 });
}

function amount(lack: Lack, value: number): string {
  return lack === "memory" ? formatMemory(value) : formatCpu(value);
}

/** La phrase « trop petit » d'après le besoin et la machine, avec sa preuve en seconde phrase. */
export function tooSmallText(
  lack: Lack,
  needed: number,
  has: number,
  proof?: "measured" | "measured_idle",
): string {
  const head = t(lack === "memory" ? "fiche.tooSmallMemory" : "fiche.tooSmallCpu", {
    needed: amount(lack, needed),
    has: amount(lack, has),
  });
  if (!proof) return head;
  const why = t(proof === "measured_idle" ? "fiche.tooSmallIdle" : "fiche.tooSmallMeasured");
  return `${head} ${why}`;
}

/** La phrase d'un constat sur un minimum de la fiche. */
export function minimumText(verdict: MinimumVerdict): string {
  switch (verdict.kind) {
    case "too_small":
      return tooSmallText(verdict.lack, verdict.needed, verdict.has, verdict.proof);
    case "publisher_above":
      return t(
        verdict.lack === "memory" ? "fiche.publisherAboveMemory" : "fiche.publisherAboveCpu",
        { needed: amount(verdict.lack, verdict.needed), has: amount(verdict.lack, verdict.has) },
      );
    case "unproven":
      return t(verdict.lack === "memory" ? "fiche.unprovenMemory" : "fiche.unprovenCpu");
  }
}

export function coverageText(coverage: Coverage): string {
  const covers = coverage.state === "covers";
  if (coverage.lack === "memory") return t(covers ? "fiche.coversMemory" : "fiche.belowMemory");
  return t(covers ? "fiche.coversCpu" : "fiche.belowCpu");
}

/** Le besoin d'espace en une phrase : le chiffre et sa preuve vivent dans la carte, ici la phrase seule. */
export function diskNeededText(bytes: number | null): string {
  return bytes === null
    ? t("fiche.diskUnproven")
    : t("fiche.diskNeeded", { size: formatSize(bytes) });
}

/** Le verdict sur la place pour installer, `null` quand tout va bien ou qu'on ne sait pas. */
export function diskText(verdict: DiskVerdict): string | null {
  switch (verdict.kind) {
    case "short_measured":
      return t("fiche.noSpaceMeasured", {
        needed: formatSize(verdict.needed),
        free: formatSize(verdict.free),
        missing: formatSize(verdict.missing),
      });
    case "short_publisher":
      return t("fiche.noSpacePublisher", {
        needed: formatSize(verdict.needed),
        free: formatSize(verdict.free),
      });
    default:
      return null;
  }
}

/** Les avertissements chiffrés de l'étape « Ressources », en direct. */
export function memoryNoticeText(notice: MemoryNotice): string {
  switch (notice.kind) {
    case "remaining":
      return notice.recommended === null
        ? t("fiche.remainingUnknown", { available: formatMemory(notice.available) })
        : t("fiche.remaining", {
            available: formatMemory(notice.available),
            recommended: formatMemory(notice.recommended),
          });
    case "over_committed":
      return t("fiche.overCommitted", { available: formatMemory(notice.available) });
    case "below_minimum":
      return t("fiche.belowMinimum");
  }
}

/** Ce que le programme télécharge au premier démarrage (BR-FICHE-010) : la phrase de la spec. */
export function downloadText(download: FicheDownload): string {
  const what = safeText(download.what);
  const source = safeText(download.source);
  const known = download.bytes !== null && proofOf(download.evidence) !== "unmeasured";
  return known && download.bytes !== null
    ? t("fiche.downloadKnown", { size: formatSize(download.bytes), what, source })
    : t("fiche.downloadUnknown", { what, source });
}

/** Les phrases de l'arrêt du jeu (BR-JEU-002), la première dit comment le monde est enregistré. */
export function stopTexts(stop: FicheStop): string[] {
  const delay = durationText(stop.graceSeconds);
  const lines: string[] = [];
  switch (stop.worldSaved) {
    case "by_image_on_signal":
      lines.push(t("fiche.stopSignal", { delay }));
      break;
    case "by_sequence":
      lines.push(t(stop.sequenceTested ? "fiche.stopSequence" : "fiche.stopUntested"));
      break;
    case "not_saved":
      lines.push(t("fiche.stopUntested"));
      break;
    default:
      lines.push(t("fiche.stopUnknown"));
  }
  lines.push(t("fiche.stopGrace", { delay }));
  return lines;
}

/** Le texte de l'adresse registre d'une image : `null` quand c'est le registre public par défaut. */
export function registryText(registry: string | null): string {
  return registry === null
    ? t("fiche.imageRegistryDefault")
    : t("fiche.imageRegistry", { registry: safeText(registry) });
}

/** La phrase d'un point d'attention propre à l'installation depuis une fiche (les autres sont ceux du déploiement). */
export function attentionText(point: AttentionPoint): string | null {
  switch (point.key) {
    case "over_committed":
      return t("fiche.attention.overCommitted", {
        chosen: formatMemory(point.chosen),
        available: formatMemory(point.available),
      });
    case "below_minimum":
      return t("fiche.attention.belowMinimum", {
        chosen: formatMemory(point.chosen),
        minimum: formatMemory(point.minimum),
      });
    case "publisher_minimum":
      return t(point.lack === "memory" ? "fiche.publisherAboveMemory" : "fiche.publisherAboveCpu", {
        needed: amount(point.lack, point.needed),
        has: amount(point.lack, point.has),
      });
    default:
      return null;
  }
}

const SETTING_REASONS: Record<string, MessageKey> = {
  bounds: "fiche.refusals.bounds",
  choice: "fiche.refusals.choice",
  length: "fiche.refusals.length",
  variant: "fiche.refusals.variant",
};

/**
 * La phrase d'un refus de l'agent propre à l'installation d'une fiche (rien n'a été créé). `detail` : la fiche lue,
 * pour nommer la question refusée par son libellé. Les autres refus sont ceux d'un déploiement (`deploy/messages.ts`).
 */
export function ficheRefusalText(
  refusal: DeployRefusal | { kind: string },
  detail: FicheDetail | null,
): string | null {
  const refused = refusal as DeployRefusal;
  switch (refused.kind) {
    case "fiche_unknown":
      return t("fiche.refusals.ficheUnknown");
    case "fiche_changed":
      return t("fiche.refusals.ficheChanged");
    case "agreement_required":
      return t("fiche.refusals.agreementRequired");
    case "answer_refused": {
      const label = detail?.questions.find((question) => question.id === refused.id)?.label;
      const head =
        label === undefined
          ? t("fiche.refusals.answerUnknown")
          : t("fiche.refusals.answer", { label: safeText(label) });
      if (refused.reason === "charset") {
        return `${head} ${charsetText(refused.character, null)}`;
      }
      return `${head} ${t(SETTING_REASONS[refused.reason] ?? "fiche.refusals.other")}`;
    }
    case "host_too_small": {
      // Un chiffre illisible (`null` sur le fil) n'est jamais remplacé par un nombre : la phrase générale suffit.
      if (refused.needed === null || refused.has === null) return t("fiche.tooSmallReason");
      const lack: Lack = refused.lack === "cpu" ? "cpu" : "memory";
      return tooSmallText(lack, refused.needed, refused.has);
    }
    case "no_space_to_install":
      if (
        refused.neededBytes === null ||
        refused.freeBytes === null ||
        refused.missingBytes === null
      ) {
        return t("fiche.noSpaceReason");
      }
      return t("fiche.noSpaceMeasured", {
        needed: formatSize(refused.neededBytes),
        free: formatSize(refused.freeBytes),
        missing: formatSize(refused.missingBytes),
      });
    default:
      return null;
  }
}

/**
 * Le refus à montrer en tête de la page d'installation, ou `null` quand le refus n'est pas propre aux fiches (le
 * parcours d'un déploiement le juge). Un refus de réponse désigne la question ; les autres ramènent à la page.
 */
export function ficheFormRefusal(
  refused: DeployRefusal,
  detail: FicheDetail | null,
): FormRefusal | null {
  if (refused.kind === "agreement_required") return null;
  const text = ficheRefusalText(refused, detail);
  if (text === null) return null;
  return {
    text,
    field: null,
    question: refused.kind === "answer_refused" ? refused.id : null,
  };
}
