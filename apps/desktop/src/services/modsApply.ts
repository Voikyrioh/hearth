import { safeText } from "@/audit/text";
import type { PendingAdd } from "@/composables/useServiceMods";
import { type MessageKey, t } from "@/i18n";
import type {
  ModItem,
  ModsApplyRefusal,
  ModsView,
  OperationStatus,
  ServiceOperation,
} from "@/link";
import { formatSize } from "@/serviceOps/messages";
import { refusalText as backupRefusalText } from "@/services/backups";
import { modName } from "@/services/mods";

/**
 * Appliquer un lot de mods (HRT-263, `BR-MOD-025` à `027`) : des fonctions pures, sans E/S. Ce que l'écran « Vérifiez
 * avant d'appliquer » dit des changements et des étapes, ce que le suivi dit de l'opération, et les phrases des refus de
 * l'agent (lus par leur CODE). Aucun texte ne vient de l'agent, sauf les lignes du journal du jeu, montrées en texte brut.
 */

/** Une ligne de « Ce qui va changer » : un signe, ce qui change, et une phrase qui dit ce que devient le fichier. */
export interface ChangeRow {
  key: string;
  sign: "+" | "~" | "-";
  title: string;
  detail: string;
  /** Le changement dit en une phrase courte, pour la barre « 1 changement prêt » : « Simple Voice Chat sera désactivé ». */
  sentence: string;
}

type Source = {
  batch: { retire: string[]; restore: string[]; disable: string[]; enable: string[] };
  adds: readonly PendingAdd[];
  view: ModsView;
};

function itemOf(view: ModsView, file: string, place: ModItem["place"]): ModItem | undefined {
  return view.mods.find((mod) => mod.file === file && mod.place === place);
}

function bareName(view: ModsView, file: string, place: ModItem["place"]): string {
  const item = itemOf(view, file, place);
  return item ? modName(item) : safeText(file);
}

function nameOf(view: ModsView, file: string, place: ModItem["place"]): string {
  const item = itemOf(view, file, place);
  const name = item ? modName(item) : safeText(file);
  const version = item?.identity?.version;
  return version ? `${name} ${safeText(version)}` : name;
}

/** Les changements du lot, dans l'ordre : ajouts et remplacements, retraits, remises, désactivations, réactivations. */
export function changeRows(source: Source): ChangeRow[] {
  const rows: ChangeRow[] = [];
  for (const add of source.adds) {
    const name = add.identity
      ? `${modName({ identity: add.identity, file: null, rank: 0, readableName: false })} ${safeText(add.identity.version ?? "")}`.trim()
      : safeText(add.targetFile);
    if (add.replace === null) {
      rows.push({
        key: `add:${add.deposit}`,
        sign: "+",
        title: t("mods.apply.add", { name }),
        detail: t("mods.apply.addDetail"),
        sentence: t("mods.bar.will.add", { name: safeText(name) }),
      });
    } else {
      const old = itemOf(source.view, add.replace, "active");
      const oldName = old ? modName(old) : safeText(add.replace);
      rows.push({
        key: `replace:${add.deposit}`,
        sign: "~",
        title: t("mods.apply.replace", {
          name: oldName,
          from: safeText(add.replaceVersion ?? ""),
          to: safeText(add.identity?.version ?? ""),
        }),
        detail: t("mods.apply.replaceDetail"),
        sentence: t("mods.bar.will.replace", {
          name: oldName,
          to: safeText(add.identity?.version ?? ""),
        }),
      });
    }
  }
  for (const file of source.batch.retire) {
    rows.push({
      key: `retire:${file}`,
      sign: "-",
      title: t("mods.apply.retire", { name: nameOf(source.view, file, "active") }),
      detail: t("mods.apply.retireDetail"),
      sentence: t("mods.bar.will.retire", { name: bareName(source.view, file, "active") }),
    });
  }
  for (const file of source.batch.restore) {
    rows.push({
      key: `restore:${file}`,
      sign: "+",
      title: t("mods.apply.restore", { name: nameOf(source.view, file, "retired") }),
      detail: t("mods.apply.restoreDetail"),
      sentence: t("mods.bar.will.restore", { name: bareName(source.view, file, "retired") }),
    });
  }
  for (const file of source.batch.disable) {
    rows.push({
      key: `disable:${file}`,
      sign: "-",
      title: t("mods.apply.disable", { name: nameOf(source.view, file, "active") }),
      detail: t("mods.apply.disableDetail"),
      sentence: t("mods.bar.will.disable", { name: bareName(source.view, file, "active") }),
    });
  }
  for (const file of source.batch.enable) {
    rows.push({
      key: `enable:${file}`,
      sign: "+",
      title: t("mods.apply.enable", { name: nameOf(source.view, file, "active") }),
      detail: t("mods.apply.enableDetail"),
      sentence: t("mods.bar.will.enable", { name: bareName(source.view, file, "active") }),
    });
  }
  return rows;
}

// ── Les étapes ───────────────────────────────────────────────────────────────────────────────────

export type StepId = "stop" | "save" | "place" | "start";

/** Les étapes que l'opération déroulera : un serveur arrêté ne passe ni par l'arrêt ni par le redémarrage (BR-MOD-025). */
export function stepsFor(running: boolean): StepId[] {
  return running ? ["stop", "save", "place", "start"] : ["save", "place"];
}

const STEP_TITLES: Record<StepId, MessageKey> = {
  stop: "mods.apply.step.stop",
  save: "mods.apply.step.save",
  place: "mods.apply.step.place",
  start: "mods.apply.step.start",
};

export function stepTitle(step: StepId): string {
  return t(STEP_TITLES[step]);
}

const SUMMARY_KEYS = ["add", "replace", "retire", "restore", "disable", "enable"] as const;

/**
 * Ce que la mise en place fait, en comptes (« 1 ajout, 1 remplacement, 1 retrait »), pour le suivi : là, aucune liste de
 * changements n'est au-dessus, « les changements ci-dessus » ne dirait rien.
 */
export function placeSummary(rows: readonly ChangeRow[]): string {
  const parts: string[] = [];
  for (const kind of SUMMARY_KEYS) {
    const n = rows.filter((row) => row.key.startsWith(`${kind}:`)).length;
    if (n > 0) parts.push(t(n === 1 ? `mods.summary.${kind}One` : `mods.summary.${kind}`, { n }));
  }
  const text = parts.join(", ");
  const retired = rows.some((row) => row.key.startsWith("retire:"));
  return retired ? t("mods.summary.withRetired", { text }) : t("mods.summary.plain", { text });
}

export function stepText(step: StepId, count: number, playersKnown: boolean): string {
  switch (step) {
    case "stop":
      return playersKnown ? t("mods.apply.stepText.stop") : t("mods.apply.stepText.stopUnknown");
    case "save":
      return t("mods.apply.stepText.save");
    case "place":
      return t(count === 1 ? "mods.apply.stepText.placeOne" : "mods.apply.stepText.place", {
        n: count,
      });
    case "start":
      return t("mods.apply.stepText.start");
  }
}

/** L'étape d'affichage où l'opération en est, d'après l'étape de l'agent (`stop`, `copy`, `verify`, `place`, `start`, `watch`). */
export function currentStep(agentStep: string | null): StepId | null {
  switch (agentStep) {
    case "stop":
      return "stop";
    case "copy":
    case "verify":
      return "save";
    case "place":
      return "place";
    case "start":
    case "watch":
      return "start";
    default:
      return null;
  }
}

export type StepState = "done" | "running" | "waiting";

/** L'état de chaque étape montrée : terminé avant l'étape courante, en cours, en attente après. */
export function stepStates(
  plan: readonly StepId[],
  operation: Pick<ServiceOperation, "step" | "status"> | null,
  succeeded: boolean,
): { step: StepId; state: StepState }[] {
  if (succeeded) return plan.map((step) => ({ step, state: "done" }));
  const current = operation ? currentStep(operation.step) : null;
  const at = current === null ? -1 : plan.indexOf(current);
  return plan.map((step, index) => ({
    step,
    state: at < 0 ? "waiting" : index < at ? "done" : index === at ? "running" : "waiting",
  }));
}

// ── Les refus de l'agent ─────────────────────────────────────────────────────────────────────────

const MOD_REFUSALS: Record<string, MessageKey> = {
  MODS_BATCH_INVALID: "mods.apply.refused.batch",
  MOD_REFUSED: "mods.apply.refused.refused",
  MOD_FILE_MISSING: "mods.apply.refused.fileMissing",
  MOD_IN_MODRINTH_PACK: "mods.apply.refused.modrinth",
  MOD_ALREADY_ACTIVE: "mods.apply.refused.alreadyActive",
  MOD_NAME_TAKEN: "mods.apply.refused.nameTaken",
  DEPOSIT_UNKNOWN: "mods.apply.refused.depositUnknown",
  MODS_NOT_APPLICABLE: "mods.apply.refused.notApplicable",
  OPERATION_IN_PROGRESS: "mods.apply.refused.busy",
  MODS_FOLDER_BLOCKED: "mods.folderBlocked",
  NOT_A_GAME_SERVICE: "mods.gone",
};

/** La phrase d'un refus de l'agent (hors confirmation, que la fenêtre dit sous son champ). */
export function applyRefusalText(refusal: ModsApplyRefusal): string {
  if (refusal.kind === "common") {
    return backupRefusalText(refusal.refusal) ?? t("mods.apply.refused.other");
  }
  return t(MOD_REFUSALS[refusal.code] ?? "mods.apply.refused.other");
}

/** L'espace qui manque pour la sauvegarde d'avant (BR-MOD-026) : dit avant, rien n'est arrêté. */
export function spaceMissingText(
  bytes: number,
  freeBytes: number | null,
  reserveBytes: number,
): string | null {
  if (freeBytes === null) return null;
  const missing = bytes + reserveBytes - freeBytes;
  return missing > 0 ? t("mods.apply.noSpace", { missing: formatSize(missing) }) : null;
}

// ── L'issue ──────────────────────────────────────────────────────────────────────────────────────

export type Outcome = "succeeded" | "rolled_back" | "cancelled" | "failed" | "attention" | "other";

export function outcomeOf(status: OperationStatus): Outcome {
  switch (status) {
    case "succeeded":
      return "succeeded";
    case "rolled_back":
      return "rolled_back";
    case "cancelled":
      return "cancelled";
    case "failed":
      return "failed";
    case "interrupted":
    case "needs_attention":
      return "attention";
    default:
      return "other";
  }
}
