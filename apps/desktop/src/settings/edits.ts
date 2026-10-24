import { type AnswerIssue, answerIssue } from "@/fiche/answers";
import type { FicheSetting, SaveSettingsRequest, SettingEffect } from "@/link";
import { isSecret, proposedOf, type SettingModel } from "./model";

/**
 * Les changements en cours d'un réglage (HRT-109, BR-FICHE-041, BR-FICHE-044, BR-FICHE-048), des fonctions pures sans E/S.
 * Un changement est l'un de trois gestes : poser une valeur (`set`), la retirer du fichier (`unset`, « non écrit »),
 * ou remettre la valeur proposée (`reset`, qui retire la ligne quand la fiche n'en propose pas). Les changements vivent
 * en mémoire SEULEMENT (une valeur peut être un mot de passe, BR-FICHE-034) : jamais dans l'adresse, un stockage ou un
 * instantané ; `wipeEdits` les écrase.
 */

export type Edit = { action: "set"; value: string } | { action: "unset" } | { action: "reset" };

export type Edits = Record<string, Edit>;

/** Efface les changements de la mémoire, autant que JavaScript le permet (une chaîne est immuable, on oublie la référence). */
export function wipeEdits(edits: Edits): void {
  for (const id of Object.keys(edits)) edits[id] = { action: "unset" };
  for (const id of Object.keys(edits)) delete edits[id];
}

/** Ce que le réglage vaut une fois le changement appliqué : écrit (avec sa valeur) ou non écrit. */
export interface Effective {
  written: boolean;
  /** La valeur voulue ; `""` quand non écrit. Un secret défini sans nouvelle saisie : `null` (jamais lu). */
  value: string | null;
  /** Un secret défini, sans nouvelle saisie. */
  secretKept: boolean;
}

export function effectiveOf(model: SettingModel, edit: Edit | undefined): Effective {
  const secret = isSecret(model.setting);
  if (edit?.action === "set") return { written: true, value: edit.value, secretKept: false };
  if (edit?.action === "unset") return { written: false, value: "", secretKept: false };
  if (edit?.action === "reset") {
    const proposed = proposedOf(model);
    return proposed === null
      ? { written: false, value: "", secretKept: false }
      : { written: true, value: proposed, secretKept: false };
  }
  if (secret) {
    return {
      written: model.state === "secret_set",
      value: null,
      secretKept: model.state === "secret_set",
    };
  }
  return model.state === "written"
    ? { written: true, value: model.value ?? "", secretKept: false }
    : { written: false, value: "", secretKept: false };
}

/**
 * La valeur que le contrôle montre : la valeur voulue, sinon — réglage non écrit — le défaut du jeu quand la fiche le
 * dit (le jeu le prend), sinon rien. Ce n'est jamais écrit tant que la personne ne touche pas au contrôle.
 */
export function controlValue(model: SettingModel, edit: Edit | undefined): string {
  const effective = effectiveOf(model, edit);
  if (effective.value !== null && effective.written) return effective.value;
  if (effective.secretKept) return "";
  return model.setting.gameDefault ?? "";
}

/**
 * Le repère « Modifié » : la valeur actuelle diffère de la valeur proposée (BR-FICHE-041). Sans valeur proposée, « non
 * écrit » est la valeur proposée : tout réglage écrit est modifié. Un secret défini est modifié.
 */
export function isModified(model: SettingModel, edit: Edit | undefined): boolean {
  const effective = effectiveOf(model, edit);
  if (isSecret(model.setting)) return effective.written;
  const proposed = proposedOf(model);
  if (proposed === null) return effective.written;
  return !(effective.written && effective.value === proposed);
}

/**
 * Le réglage peut-il être remis à la valeur proposée par une remise EN BLOC (BR-FICHE-044) ? Ni un réglage figé ou
 * imposé ou retiré, ni un secret, ni un réglage qui attend la recréation ; et seulement s'il diffère de la proposée.
 */
export function resettable(model: SettingModel, edit: Edit | undefined): boolean {
  if (model.lock !== null || isSecret(model.setting)) return false;
  return isModified(model, edit);
}

/** Le contrôle du réglage est en lecture seule. */
export function readOnly(model: SettingModel): boolean {
  return model.lock !== null;
}

/** La phrase du défaut d'une valeur à saisir : `null` si elle est permise (le serveur reste l'arbitre). */
export function valueIssue(
  setting: FicheSetting,
  value: string,
): AnswerIssue | { code: "list" } | null {
  if (setting.question.kind === "id_list")
    return listIssue(setting, value) ? { code: "list" } : null;
  return answerIssue(setting.question, value);
}

/** Les identifiants d'une liste, un par ligne ou séparés par le séparateur de la fiche, sans ligne vide. */
export function listItems(text: string): string[] {
  return text
    .split(/[\r\n]+/u)
    .map((item) => item.trim())
    .filter((item) => item !== "");
}

/** La liste d'identifiants telle que le jeu la lit : les éléments joints par le séparateur de la fiche. */
export function listValue(setting: FicheSetting, text: string): string {
  return listItems(text).join(setting.list?.separator ?? ",");
}

/** La liste telle que la personne la lit : un identifiant par ligne. */
export function listText(setting: FicheSetting, value: string): string {
  const separator = setting.list?.separator ?? ",";
  return value === "" ? "" : value.split(separator).join("\n");
}

const DIGITS = /^[0-9]+$/;
const SLUG = /^[a-z0-9_-]+$/;
const MAX_ITEM_CHARS = 64;

/** Le défaut d'une liste (nombre, jeu de caractères, longueur d'un identifiant) ; `null` : la liste est permise. */
export function listIssue(
  setting: FicheSetting,
  text: string,
):
  | { code: "too_many"; max: number }
  | { code: "bad_item"; item: string }
  | { code: "too_long" }
  | null {
  const list = setting.list;
  const items = listItems(text);
  if (list && items.length > list.maxItems) return { code: "too_many", max: list.maxItems };
  for (const item of items) {
    if ([...item].length > MAX_ITEM_CHARS) return { code: "too_long" };
    if (list?.itemCharset === "digits" && !DIGITS.test(item)) return { code: "bad_item", item };
    if (list?.itemCharset === "slug" && !SLUG.test(item)) return { code: "bad_item", item };
  }
  return null;
}

/** Les changements qui ont un effet : un geste qui ne change rien à ce que le réglage vaut déjà n'est pas envoyé. */
export function pendingEdits(models: readonly SettingModel[], edits: Edits): SettingModel[] {
  return models.filter((model) => {
    const edit = edits[model.id];
    if (!edit || model.lock !== null) return false;
    if (isSecret(model.setting)) {
      // Un secret ne se compare pas : poser est un changement ; retirer n'en est un que s'il est défini.
      if (edit.action === "set") return true;
      if (edit.action === "unset") return model.state === "secret_set";
      return false;
    }
    const before = effectiveOf(model, undefined);
    const after = effectiveOf(model, edit);
    return before.written !== after.written || before.value !== after.value;
  });
}

/** Le défaut de forme de chaque changement posé, par identifiant ; un réglage sans défaut n'y figure pas. */
export function editIssues(
  models: readonly SettingModel[],
  edits: Edits,
): Map<string, AnswerIssue | { code: "list" }> {
  const found = new Map<string, AnswerIssue | { code: "list" }>();
  for (const model of models) {
    const edit = edits[model.id];
    if (edit?.action !== "set" || model.lock !== null) continue;
    const issue = valueIssue(model.setting, edit.value);
    if (issue) found.set(model.id, issue);
  }
  return found;
}

const WEIGHT: Record<SettingEffect, number> = { none: 0, restart: 1, unknown: 1, recreate: 2 };

/** L'effet le plus lourd de ces réglages (BR-FICHE-042) : recréation avant redémarrage avant rien. */
export function heaviestEffect(models: readonly SettingModel[]): SettingEffect {
  let heaviest: SettingEffect = "none";
  for (const model of models) {
    if (WEIGHT[model.effect] > WEIGHT[heaviest])
      heaviest = model.effect === "unknown" ? "restart" : model.effect;
  }
  return heaviest;
}

export type DriftChoices = Record<string, "keep_file" | "keep_mine">;

/**
 * La demande d'enregistrement du formulaire (BR-FICHE-041, BR-FICHE-048) : les changements posés qui ont un effet, les
 * choix d'écart (un par réglage, « Garder la valeur du fichier » par défaut), les réglages inconnus modifiés ou retirés.
 * `fileStamp` et `baseRev` sont ceux de la lecture : un fichier qui a bougé depuis est refusé par l'agent. L'origine
 * (signée) : un écart tranché seul, une remise aux valeurs proposées, ou le formulaire.
 */
export function toSaveRequest(input: {
  models: readonly SettingModel[];
  edits: Edits;
  drifts: readonly { id: string }[];
  choices: DriftChoices;
  others: readonly { key: string; remove: boolean; value: string | null }[];
  baseRev: number;
  fileStamp: string;
}): { request: SaveSettingsRequest; effect: SettingEffect; changed: SettingModel[] } {
  const changed = pendingEdits(input.models, input.edits);
  const keepMine = input.drifts.some((drift) => input.choices[drift.id] === "keep_mine");
  // Le fichier est retouché dès qu'un réglage, un réglage inconnu ou un écart rendu à Hearth change quelque chose ;
  // garder la valeur du fichier ne touche à rien (l'agent recalcule et refuse un effet qui diffère).
  const base = heaviestEffect(changed);
  const effect: SettingEffect =
    base === "none" && (input.others.length > 0 || keepMine) ? "restart" : base;
  const resetOnly =
    changed.length > 0 && changed.every((model) => input.edits[model.id]?.action === "reset");
  const origin =
    changed.length === 0 && input.others.length === 0 && input.drifts.length > 0
      ? "drift"
      : resetOnly && input.others.length === 0
        ? "reset"
        : "form";
  const request: SaveSettingsRequest = {
    baseRev: input.baseRev,
    fileStamp: input.fileStamp,
    origin,
    effect,
    changes: changed.map((model) => {
      const edit = input.edits[model.id] as Edit;
      return {
        id: model.id,
        action: edit.action,
        value: edit.action === "set" ? edit.value : null,
      };
    }),
    others: input.others.map((other) => ({
      key: other.key,
      remove: other.remove,
      value: other.remove ? null : other.value,
    })),
    driftChoices: input.drifts.map((drift) => ({
      id: drift.id,
      choice: input.choices[drift.id] ?? "keep_file",
    })),
    importText: null,
  };
  return { request, effect, changed };
}

/** Rien n'est à enregistrer : aucun changement, aucun réglage inconnu touché, aucun écart à trancher. */
export function nothingToSave(input: {
  changed: readonly SettingModel[];
  others: readonly unknown[];
  drifts: readonly unknown[];
}): boolean {
  return input.changed.length === 0 && input.others.length === 0 && input.drifts.length === 0;
}
