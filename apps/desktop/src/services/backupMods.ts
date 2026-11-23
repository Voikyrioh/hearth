import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type {
  BackupModItem,
  BackupMods,
  BackupModsState,
  BackupModsSummary,
  ChangeKind,
  ModChange,
} from "@/link";
import { dateOf } from "@/services/backups";
import { loaderName } from "@/services/mods";
import { copyText, entryOf } from "@/services/modsCopy";

/**
 * Les mods de chaque sauvegarde (HRT-294, `BR-MOD-034` à `036`) : des fonctions pures. L'agent enregistre la liste et
 * calcule la comparaison avec les mods actuels ; ici, seulement les mots de la colonne « Mods », le tri par nom, les
 * phrases des états sans liste et le regroupement des changements par genre. Un nom est un texte inoffensif.
 */

/** Le mot de la colonne « Mods » (BR-MOD-035) : « 5 actifs, 1 désactivé », ou un mot court quand il n'y a pas de liste. */
export function columnText(summary: BackupModsSummary): string {
  switch (summary.state) {
    case "recorded":
    case "partial": {
      const active = t(summary.active === 1 ? "backups.mods.activeOne" : "backups.mods.active", {
        n: summary.active,
      });
      if (summary.disabled === 0) return active;
      const disabled = t(
        summary.disabled === 1 ? "backups.mods.disabledOne" : "backups.mods.disabled",
        { n: summary.disabled },
      );
      return `${active}, ${disabled}`;
    }
    case "not_recorded":
      return t("backups.mods.notRecorded");
    case "unreadable":
      return t("backups.mods.unreadable");
    case "not_in_backup":
      return t("backups.mods.notInBackup");
    default:
      return t("backups.mods.unknown");
  }
}

/** La phrase d'un état sans liste lisible (BR-MOD-034) ; `null` : une liste existe. */
export function stateSentence(state: BackupModsState): string | null {
  switch (state) {
    case "recorded":
      return null;
    case "partial":
      return t("backups.mods.sentence.partial");
    case "unreadable":
      return t("backups.mods.sentence.unreadable");
    case "not_in_backup":
      return t("backups.mods.sentence.notInBackup");
    case "not_recorded":
      return t("backups.mods.sentence.notRecorded");
    default:
      return t("backups.mods.sentence.unknown");
  }
}

/** Une liste est montrée : enregistrée, ou enregistrée en partie. */
export function hasList(state: BackupModsState): boolean {
  return state === "recorded" || state === "partial";
}

/** Le nom montré d'un fichier de la sauvegarde : le mod, sinon « Non reconnu dans cette sauvegarde » avec son fichier. */
export function itemName(item: BackupModItem): string {
  if (item.identity) return safeText(item.identity.name ?? item.identity.id);
  return t("backups.mods.unrecognized");
}

export function itemFile(item: BackupModItem): string | null {
  return item.file === null ? null : safeText(item.file);
}

/** Le rang d'un fichier au nom illisible (jamais son nom). */
export function itemRank(item: BackupModItem): string | null {
  return item.file === null ? t("mods.unreadableName", { rank: item.rank + 1 }) : null;
}

/** Les mods de la liste, triés par nom (BR-MOD-035), hors « Mods retirés ». */
export function listed(view: BackupMods): BackupModItem[] {
  return view.mods
    .filter((item) => item.place !== "retired")
    .sort((a, b) => itemName(a).localeCompare(itemName(b), "fr", { sensitivity: "base" }));
}

export function retiredCount(view: BackupMods): number {
  return view.mods.filter((item) => item.place === "retired").length;
}

const PLACE_KEYS: Record<string, MessageKey> = {
  active: "backups.mods.place.active",
  disabled: "backups.mods.place.disabled",
  retired: "backups.mods.place.retired",
};

export function placeText(item: BackupModItem): string {
  return t(PLACE_KEYS[item.place] ?? "backups.mods.place.unknown");
}

export function engineLine(view: BackupMods): string {
  return t("backups.mods.engine", {
    loader: loaderName(view.game.loader),
    minecraft: view.game.minecraft ?? "?",
  });
}

/** « Copier la liste » : les mods ACTIVÉS de la sauvegarde, au format de BR-MOD-029, sous « Mods de la sauvegarde du {date} ». */
export function copyOf(view: BackupMods, createdAt: string, address: string | null): string {
  return copyText({
    title: t("backups.mods.copyTitle", { date: dateOf(createdAt) }),
    address,
    minecraft: view.game.minecraft,
    entries: view.mods
      .flatMap((item) => (item.place === "active" && item.identity ? [entryOf(item.identity)] : []))
      .sort((a, b) => a.name.localeCompare(b.name, "fr", { sensitivity: "base" })),
  });
}

// ── La comparaison ───────────────────────────────────────────────────────────────────────────────

const GROUPS: { kind: ChangeKind; title: MessageKey }[] = [
  { kind: "added", title: "backups.mods.compare.added" },
  { kind: "removed", title: "backups.mods.compare.removed" },
  { kind: "version_changed", title: "backups.mods.compare.versionChanged" },
  { kind: "enabled", title: "backups.mods.compare.enabled" },
  { kind: "disabled", title: "backups.mods.compare.disabled" },
];

export interface ChangeGroup {
  kind: ChangeKind;
  title: string;
  lines: string[];
}

function changeName(change: ModChange): string {
  return safeText(change.name ?? change.id ?? change.file ?? "?");
}

/** Les changements regroupés par genre, dans l'ordre : ajouté, retiré, version changée, activé, désactivé. */
export function groupChanges(changes: readonly ModChange[]): ChangeGroup[] {
  return GROUPS.flatMap(({ kind, title }) => {
    const lines = changes
      .filter((change) => change.kind === kind)
      .map((change) =>
        kind === "version_changed"
          ? t("backups.mods.compare.versionLine", {
              name: changeName(change),
              backup: safeText(change.backupVersion ?? "?"),
              current: safeText(change.currentVersion ?? "?"),
            })
          : changeName(change),
      );
    return lines.length === 0 ? [] : [{ kind, title: t(title), lines }];
  });
}
