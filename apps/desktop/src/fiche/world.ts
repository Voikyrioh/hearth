import { safeText } from "@/audit/text";
import { formatDateTime } from "@/composables/format";
import { type MessageKey, t } from "@/i18n";
import type {
  FicheDetail,
  WorldEntry,
  WorldInspection,
  WorldPick,
  WorldRefusal,
  WorldsView,
} from "@/link";
import { formatCount, formatSize } from "@/serviceOps/messages";

/**
 * Reprendre un monde existant (HRT-106, ADR-0059, `BR-MONDE-*`) : des fonctions pures, sans E/S. L'agent reste l'arbitre
 * (dossier ouvert, chemin, nom accepté, place, geste « Je comprends ») ; ceci dit d'avance, honnêtement, si le choix est
 * offert, ce que la liste montre et ce que la copie fera. Un nom de monde, un dossier ou un nom d'élément vient du
 * serveur : toujours rendu inoffensif (`safeText`), jamais écrit dans une trace.
 */

/** La saisie de la carte « Monde » (en mémoire seulement, comme le reste du brouillon). */
export interface WorldDraft {
  /** « Reprendre un monde existant » est choisi. */
  resume: boolean;
  /** Le monde choisi : un dossier ouvert aux services et un chemin relatif. */
  folder: string | null;
  path: string | null;
  /** Le nom du monde tel que la liste le montre (pour le récapitulatif ; jamais envoyé : l'agent le relit). */
  name: string | null;
  /** « Garder cette copie » d'avance si des fichiers changent pendant la copie (BR-MONDE-009). */
  keepIfChanged: boolean;
}

export function newWorldDraft(): WorldDraft {
  return { resume: false, folder: null, path: null, name: null, keepIfChanged: false };
}

export function wipeWorldDraft(world: WorldDraft): void {
  world.resume = false;
  world.folder = null;
  world.path = null;
  world.name = null;
  world.keepIfChanged = false;
}

/** La fiche sait-elle reprendre un monde (elle dit quel réglage reçoit le nom du monde) ? */
export function ficheKnowsWorlds(detail: Pick<FicheDetail, "worldImport">): boolean {
  return detail.worldImport !== null;
}

/**
 * Les réglages que le monde repris porte : le nom (rempli par l'agent) et ceux que le monde décide. Le client ne pose
 * aucune réponse pour eux (BR-MONDE-014) : ils ne sont ni posés ni envoyés.
 */
export function hiddenByWorld(
  detail: Pick<FicheDetail, "worldImport">,
  world: Pick<WorldDraft, "resume">,
): ReadonlySet<string> {
  if (!world.resume || detail.worldImport === null) return new Set();
  return new Set([detail.worldImport.namesSetting, ...detail.worldImport.carriedByWorld]);
}

/** Le monde est-il choisi (dossier et chemin) ? */
export function worldChosen(world: WorldDraft): boolean {
  return world.resume && world.folder !== null && world.path !== null;
}

/** Une reprise est demandée mais aucun monde n'est choisi : la page ne continue pas (« Choisissez un monde à reprendre »). */
export function worldMissing(world: WorldDraft): boolean {
  return world.resume && !worldChosen(world);
}

/** Le monde que la demande d'installation porte, `null` : un monde neuf (ou une reprise pas prête). */
export function worldPickOf(world: WorldDraft, versionUnderstood: boolean): WorldPick | null {
  if (!worldChosen(world) || world.folder === null || world.path === null) return null;
  return {
    folder: world.folder,
    path: world.path,
    versionUnderstood,
    keepIfChanged: world.keepIfChanged,
  };
}

/** Ce que la carte « Monde » propose, d'après la lecture. */
export type WorldChoice =
  /** Aucun dossier du serveur n'est ouvert aux services (BR-MONDE-002) : le choix est désactivé. */
  | { kind: "no_folder" }
  /** Des dossiers sont ouverts : la liste peut être vide, ou tronquée (BR-MONDE-003). */
  | { kind: "folders"; truncated: boolean; unreadable: number };

export function worldChoice(view: WorldsView): WorldChoice {
  if (view.roots.length === 0) return { kind: "no_folder" };
  return {
    kind: "folders",
    truncated: view.roots.some((root) => root.truncated),
    unreadable: view.roots.filter((root) => !root.readable).length,
  };
}

/** Un monde dans la liste, avec son dossier : deux mondes de même nom dans deux dossiers sont deux lignes (BR-MONDE-003). */
export interface WorldRow {
  folder: string;
  entry: WorldEntry;
}

export function worldRows(view: WorldsView): WorldRow[] {
  return view.roots.flatMap((root) => root.worlds.map((entry) => ({ folder: root.folder, entry })));
}

export const sameWorld = (row: WorldRow, world: Pick<WorldDraft, "folder" | "path">): boolean =>
  row.folder === world.folder && row.entry.path === world.path;

// ── Les textes ───────────────────────────────────────────────────────────────────────────────

export function worldName(entry: Pick<WorldEntry, "name">): string {
  return safeText(entry.name);
}

export function folderText(folder: string): string {
  return safeText(folder);
}

/** « 1 fichier » / « 12 fichiers » ; `null` : un monde « dossier », mesuré seulement par l'inspection. */
export function filesText(files: number | null): string {
  if (files === null) return t("fiche.world.unmeasured");
  return files === 1
    ? t("fiche.world.fileOne")
    : t("fiche.world.files", { count: formatCount(files) });
}

export function sizeText(bytes: number | null): string {
  return bytes === null ? t("fiche.world.unmeasured") : formatSize(bytes);
}

export function dateText(iso: string | null): string {
  return iso === null ? t("fiche.world.unmeasured") : formatDateTime(iso);
}

/** « Au moins » quand la mesure s'est arrêtée à son budget (BR-MONDE-006). */
export function inspectionSize(inspection: Pick<WorldInspection, "bytes" | "partial">): string {
  const size = formatSize(inspection.bytes);
  return inspection.partial ? t("fiche.world.atLeast", { size }) : size;
}

export function inspectionFiles(inspection: Pick<WorldInspection, "files" | "partial">): string {
  const files = filesText(inspection.files);
  return inspection.partial ? t("fiche.world.atLeast", { size: files }) : files;
}

/** Les noms cités (50 au plus côté agent), rendus inoffensifs et reliés. */
export function namesText(names: readonly string[], total: number): string {
  const shown = names.map((name) => safeText(name)).join(", ");
  return total > names.length
    ? t("fiche.world.andMore", { list: shown, n: total - names.length })
    : shown;
}

/** La ligne des éléments qui ne seront pas copiés (sauvegardes, liens, fichiers spéciaux), ou `null` (BR-MONDE-005, 006). */
export function notCopiedLines(inspection: WorldInspection): string[] {
  const lines: string[] = [];
  if (inspection.notCopied > 0) {
    lines.push(
      t("fiche.world.notCopied", {
        count: formatCount(inspection.notCopied),
        list: namesText(inspection.notCopiedNames, inspection.notCopied),
      }),
    );
  }
  if (inspection.skippedLinks > 0) {
    lines.push(
      t("fiche.world.skipped", {
        count: formatCount(inspection.skippedLinks),
        list: namesText(inspection.skippedLinkNames, inspection.skippedLinks),
      }),
    );
  }
  return lines;
}

/** Il faudra plus de place que le disque n'en a : dit avant le lancement, sans case pour passer outre (BR-MONDE-012). */
export function notEnoughRoom(inspection: Pick<WorldInspection, "bytes" | "freeBytes">): boolean {
  return inspection.freeBytes !== null && inspection.bytes > inspection.freeBytes;
}

const REFUSALS: Record<WorldRefusal, MessageKey> = {
  unknown_folder: "fiche.world.refusals.unknownFolder",
  bad_path: "fiche.world.refusals.badPath",
  not_a_world: "fiche.world.refusals.notAWorld",
  not_found: "fiche.world.refusals.notFound",
  too_big: "fiche.world.refusals.tooBig",
  name_unusable: "fiche.world.refusals.nameUnusable",
  unreadable: "fiche.world.refusals.unreadable",
  unknown: "fiche.world.refusals.other",
};

/** Pourquoi l'agent refuse ce monde ; un code que ce client ne connaît pas dit le refus général. */
export function refusalText(reason: WorldRefusal): string {
  return t(REFUSALS[reason] ?? "fiche.world.refusals.other");
}

/** Les raisons d'un refus d'installation propres au monde (`details.reason` de `source.world`, `source`). */
const INSTALL_REASONS: Record<string, MessageKey> = {
  unavailable: "fiche.world.refusals.unavailable",
  unknown_folder: "fiche.world.refusals.unknownFolder",
  bad_path: "fiche.world.refusals.badPath",
  not_a_world: "fiche.world.refusals.notAWorld",
  not_found: "fiche.world.refusals.gone",
  too_big: "fiche.world.refusals.tooBig",
  name_unusable: "fiche.world.refusals.nameUnusable",
  not_supported: "fiche.world.refusals.notSupported",
  version_not_understood: "fiche.world.refusals.versionNotUnderstood",
  world_import_unavailable: "fiche.world.refusals.unavailable",
  world_import_inconsistent: "fiche.world.refusals.other",
};

/** La phrase d'un refus d'installation qui vise le monde, ou `null` : ce refus n'est pas celui du monde. */
export function installRefusalText(refusal: {
  kind: string;
  field?: string;
  reason?: string;
}): string | null {
  if (refusal.kind === "heavy_io_busy") return t("fiche.world.refusals.heavyIo");
  if (refusal.kind !== "invalid" || (refusal.field !== "world" && refusal.field !== "source")) {
    return null;
  }
  return t(INSTALL_REASONS[refusal.reason ?? ""] ?? "fiche.world.refusals.other");
}

/** Le refus « le nom du monde est fixé » : l'agent pose lui-même le nom du monde repris (BR-MONDE-014). */
export function nameFixedRefusal(refusal: {
  kind: string;
  reason?: string;
  id?: string | null;
}): boolean {
  return refusal.kind === "answer_refused" && refusal.reason === "world_name_fixed";
}
