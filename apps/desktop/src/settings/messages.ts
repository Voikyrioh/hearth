import { safeText } from "@/audit/text";
import type { AnswerIssue } from "@/fiche/answers";
import { answerIssueText } from "@/fiche/messages";
import { type MessageKey, t } from "@/i18n";
import type { ImportState, SaveRefusal, SettingEffect } from "@/link";
import type { listIssue } from "./edits";
import { isSecret, type SettingModel } from "./model";

/**
 * Les textes de l'onglet « Réglages » (HRT-109), des fonctions pures : les CODES des règles (`edits.ts`, `filter.ts`) et de
 * l'agent mis en phrases, vouvoiement et impératif, jamais un texte de l'agent, jamais la valeur d'un réglage (sauf UN
 * caractère refusé, comme l'agent). Les libellés, aides, bornes et choix viennent de la fiche ; seules les phrases autour
 * sont d'ici.
 */

/** Une valeur dite pour une personne : le libellé du choix, « vide » pour un texte vide, « non écrit » sinon. */
export function valueText(model: SettingModel, value: string | null): string {
  if (isSecret(model.setting))
    return value === null ? t("gameSettings.valueNotWritten") : t("gameSettings.valueSecret");
  if (value === null) return t("gameSettings.valueNotWritten");
  if (value === "") return t("gameSettings.valueEmpty");
  const question = model.setting.question;
  if (question.kind === "bool") {
    return value === question.choices[0]?.value ? t("fiche.yes") : t("fiche.no");
  }
  const choice = question.choices.find((item) => item.value === value);
  return safeText(choice ? choice.label : value);
}

/** La ligne « Défaut du jeu » d'un réglage ; `null` : un secret n'a pas de défaut à dire. */
export function defaultLine(model: SettingModel): string | null {
  if (isSecret(model.setting)) return null;
  const value = model.setting.gameDefault;
  if (value === null) return t("gameSettings.gameDefaultUnverified");
  if (value === "") return t("gameSettings.gameDefaultEmpty");
  return t("gameSettings.gameDefault", { value: valueText(model, value) });
}

/** La ligne « Valeur proposée », seulement quand elle diffère du défaut du jeu (design 16.2) ; sinon `null`. */
export function proposedLine(model: SettingModel): string | null {
  if (isSecret(model.setting)) return null;
  const proposed = model.setting.question.proposed;
  if (proposed === null || proposed === model.setting.gameDefault) return null;
  if (proposed === "") return t("gameSettings.proposedEmpty");
  return t("gameSettings.proposed", { value: valueText(model, proposed) });
}

/**
 * La mention d'effet d'un réglage : l'une des quatre de BR-FICHE-042, jamais d'autre. Un réglage verrouillé dit pourquoi
 * ailleurs (sa phrase) ; `null` : rien à dire (aucun effet).
 */
export function effectText(effect: SettingEffect, frozen: boolean): string | null {
  if (frozen) return t("gameSettings.effect.frozen");
  switch (effect) {
    case "restart":
      return t("gameSettings.effect.restart");
    case "recreate":
      return t("gameSettings.effect.recreate");
    case "unknown":
      return t("gameSettings.effect.unknown");
    case "none":
      return null;
  }
}

/** Le défaut d'une valeur à saisir, en phrase. */
export function issueText(issue: AnswerIssue | { code: "list" }): string {
  if (issue.code === "list") return t("gameSettings.listIssue.other", { item: "" }).trim();
  return answerIssueText(issue);
}

export function listIssueText(
  issue: NonNullable<ReturnType<typeof listIssue>>,
  charset: string,
): string {
  switch (issue.code) {
    case "too_many":
      return t("gameSettings.listIssue.tooMany", { max: issue.max });
    case "too_long":
      return t("gameSettings.listIssue.tooLong");
    case "bad_item": {
      const item = safeText(issue.item);
      if (charset === "digits") return t("gameSettings.listIssue.digits", { item });
      if (charset === "slug") return t("gameSettings.listIssue.slug", { item });
      return t("gameSettings.listIssue.other", { item });
    }
  }
}

const REFUSED_REASONS: Record<string, MessageKey> = {
  bounds: "gameSettings.refusedReason.bounds",
  choice: "gameSettings.refusedReason.choice",
  charset: "gameSettings.refusedReason.charset",
  length: "gameSettings.refusedReason.length",
  frozen: "gameSettings.refusedReason.frozen",
  imposed: "gameSettings.refusedReason.imposed",
  retired: "gameSettings.refusedReason.retired",
  unknown_id: "gameSettings.refusedReason.unknown_id",
  variant: "gameSettings.refusedReason.variant",
  version: "gameSettings.refusedReason.version",
  check: "gameSettings.refusedReason.check",
  secret: "gameSettings.refusedReason.secret",
  carried_by_world: "gameSettings.refusedReason.carried_by_world",
};

/** Le refus d'un réglage mis en phrase : le libellé de la FICHE (jamais un nom venu d'ailleurs), la raison, au plus UN caractère. */
export function settingRefusalText(
  refusal: Extract<SaveRefusal, { kind: "setting_refused" }>,
  labelOf: (id: string) => string | null,
): string {
  const key = REFUSED_REASONS[refusal.reason];
  let why: string;
  if (refusal.reason === "charset" && refusal.character !== null) {
    why = t("gameSettings.refusedReason.charsetChar", { character: safeText(refusal.character) });
  } else {
    why = t(key ?? "gameSettings.refusedReason.other");
  }
  const label = refusal.id === null ? null : labelOf(refusal.id);
  return label === null
    ? t("gameSettings.saveRefusals.settingUnknown", { why })
    : t("gameSettings.saveRefusals.setting", { label: safeText(label), why });
}

/** Le refus d'un enregistrement, en phrase ; `undefined` : un refus de la confirmation, que la fenêtre dit elle-même. */
export function saveRefusalText(
  refusal: SaveRefusal,
  labelOf: (id: string) => string | null,
): string | undefined {
  switch (refusal.kind) {
    case "wrong_password":
    case "password_required":
    case "too_many_attempts":
    case "busy":
      return undefined;
    case "conflict":
      if (refusal.effectMismatch) return t("gameSettings.saveRefusals.effect");
      return refusal.by === null
        ? t("gameSettings.saveRefusals.conflict")
        : t("gameSettings.saveRefusals.conflictBy", { by: safeText(refusal.by) });
    case "file_changed":
      return t("gameSettings.saveRefusals.fileChanged");
    case "file_unreadable":
      return refusal.line === null
        ? t("gameSettings.saveRefusals.fileUnreadable")
        : t("gameSettings.saveRefusals.fileUnreadableLine", { line: refusal.line });
    case "setting_refused":
      return settingRefusalText(refusal, labelOf);
    case "recreate_not_available":
      return t("gameSettings.saveRefusals.recreate");
    case "operation_in_progress":
      return t("gameSettings.saveRefusals.busy");
    case "service_not_found":
      return t("gameSettings.saveRefusals.notFound");
    case "invalid":
      return t("gameSettings.saveRefusals.invalid");
    case "other":
      return t("gameSettings.saveRefusals.other");
  }
}

const IMPORT_STATES: Record<ImportState, MessageKey> = {
  take: "gameSettings.importStates.take",
  same: "gameSettings.importStates.same",
  rejected: "gameSettings.importStates.rejected",
  imposed: "gameSettings.importStates.imposed",
  frozen: "gameSettings.importStates.frozen",
  unknown: "gameSettings.importStates.unknown",
};

export function importStateText(state: ImportState): string {
  return t(IMPORT_STATES[state]);
}
