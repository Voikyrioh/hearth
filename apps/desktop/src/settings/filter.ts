import type { FicheTheme } from "@/link";
import { type Edits, isModified } from "./edits";
import type { SettingModel } from "./model";

/**
 * Ranger, chercher et filtrer les réglages (HRT-109, BR-FICHE-041, BR-FICHE-043, BR-FICHE-045), des fonctions pures sans
 * E/S. Les thèmes sont ceux de la fiche, dans l'ordre de la fiche ; le client n'en invente aucun.
 */

/** Le texte sans majuscules ni accents, pour chercher (BR-FICHE-043). */
export function plain(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

/**
 * Le réglage répond à la recherche : par libellé, nom technique ou phrase d'explication, sans tenir compte des
 * majuscules ni des accents, sur tous les thèmes et tous les niveaux. Une recherche vide répond toujours.
 */
export function matches(model: SettingModel, query: string): boolean {
  const wanted = plain(query.trim());
  if (wanted === "") return true;
  const { question, key } = model.setting;
  return [question.label, key, question.id, question.help].some((text) =>
    plain(text).includes(wanted),
  );
}

/** Les deux vues de l'onglet : « Courants » (essentiels et courants) ou « Tout afficher » (BR-FICHE-045). */
export type Scope = "common" | "all";

export interface Filter {
  query: string;
  modifiedOnly: boolean;
  scope: Scope;
}

/** Le réglage est de la vue « Courants » : essentiel ou courant. Un niveau inconnu est avancé. */
export function isCommon(model: SettingModel): boolean {
  return model.setting.level === "essential" || model.setting.level === "common";
}

/**
 * Les réglages montrés. La recherche cherche toujours dans tous les niveaux (BR-FICHE-045) ; sans recherche, la vue
 * « Courants » n'a que les essentiels et les courants. Le filtre « Modifiés » ne garde que ce qui diffère de la valeur
 * proposée. Un réglage « modifié » mais avancé reste caché en vue « Courants » : le compteur du thème le dit.
 */
export function shown(
  models: readonly SettingModel[],
  filter: Filter,
  edits: Edits,
): SettingModel[] {
  const searching = filter.query.trim() !== "";
  return models.filter((model) => {
    if (!matches(model, filter.query)) return false;
    if (filter.modifiedOnly && !isModified(model, edits[model.id])) return false;
    if (!searching && filter.scope === "common" && !isCommon(model)) return false;
    return true;
  });
}

export interface ThemeGroup {
  theme: FicheTheme;
  models: SettingModel[];
  /** Combien de réglages le thème compte en tout, et combien sont modifiés (le compteur de la carte). */
  total: number;
  modified: number;
}

/**
 * Les thèmes avec leurs réglages montrés, dans l'ordre de la fiche. Un thème sans réglage montré disparaît ; un réglage
 * dont le thème n'est pas dans la liste est rangé à la fin dans un thème anonyme (jamais perdu).
 */
export function groupByTheme(
  themes: readonly FicheTheme[],
  all: readonly SettingModel[],
  visible: readonly SettingModel[],
  edits: Edits,
): ThemeGroup[] {
  const known = new Set(themes.map((theme) => theme.id));
  const ordered: FicheTheme[] = [...themes].sort((a, b) => a.order - b.order);
  const stray = all.filter((model) => !known.has(model.setting.theme));
  if (stray.length > 0) ordered.push({ id: "", label: "", order: Number.MAX_SAFE_INTEGER });
  const groups: ThemeGroup[] = [];
  for (const theme of ordered) {
    const inTheme = (model: SettingModel) =>
      theme.id === "" ? !known.has(model.setting.theme) : model.setting.theme === theme.id;
    const members = visible.filter(inTheme);
    if (members.length === 0) continue;
    const mine = all.filter(inTheme);
    groups.push({
      theme,
      models: members,
      total: mine.length,
      modified: mine.filter((model) => isModified(model, edits[model.id])).length,
    });
  }
  return groups;
}

/** Un thème s'ouvre par défaut s'il contient un réglage essentiel ou modifié (BR-FICHE-041). */
export function opensByDefault(group: ThemeGroup): boolean {
  return group.modified > 0 || group.models.some((model) => model.setting.level === "essential");
}
