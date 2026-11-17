import type { FicheCard } from "@/link";

/**
 * Le rangement du Catalogue (HRT-241, BR-FICHE-063), des fonctions pures sans E/S : deux sections, « Prêts à installer »
 * puis « Bientôt dans Hearth », chacune triée par nom ; la recherche garde les noms qui contiennent le texte, sans tenir
 * compte des majuscules ni des accents ; les filtres portent le nombre de cartes visibles avec la recherche en cours.
 * Une fiche est « prête » quand son état est `published`, « bientôt » quand elle est en cours de preuve (`preview`) ;
 * un état inconnu d'une version plus récente se range avec les fiches prêtes seulement s'il est installable.
 */

export type CatalogFilter = "all" | "ready" | "soon";

export interface CatalogSections {
  ready: FicheCard[];
  soon: FicheCard[];
}

/** Longueur maximale du texte cherché (BR-FICHE-063). */
export const SEARCH_MAX = 60;

/** Minuscules et sans signes diacritiques : « Éléphant » et « elephant » se valent. */
export function normalize(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Une fiche qui attend (« Bientôt ») : en cours de preuve et non installable. */
export function isSoon(card: FicheCard): boolean {
  return card.status === "preview" && !card.installable;
}

function byName(a: FicheCard, b: FicheCard): number {
  return a.name.localeCompare(b.name, "fr", { sensitivity: "base" });
}

function matches(card: FicheCard, query: string): boolean {
  const needle = normalize(query.trim());
  return needle === "" || normalize(card.name).includes(needle);
}

/** Les deux sections visibles pour cette recherche et ce filtre. */
export function sectionsOf(
  fiches: readonly FicheCard[],
  query: string,
  filter: CatalogFilter,
): CatalogSections {
  const kept = fiches.filter((card) => matches(card, query));
  const ready = kept.filter((card) => !isSoon(card)).sort(byName);
  const soon = kept.filter(isSoon).sort(byName);
  return {
    ready: filter === "soon" ? [] : ready,
    soon: filter === "ready" ? [] : soon,
  };
}

/** Le nombre de cartes de chaque filtre, avec la recherche en cours. */
export function counts(
  fiches: readonly FicheCard[],
  query: string,
): { all: number; ready: number; soon: number } {
  const all = sectionsOf(fiches, query, "all");
  return {
    all: all.ready.length + all.soon.length,
    ready: all.ready.length,
    soon: all.soon.length,
  };
}

/** Le filtre « Bientôt » n'existe que s'il y a au moins une fiche qui attend dans le catalogue de l'agent (BR-FICHE-063). */
export function hasSoon(fiches: readonly FicheCard[]): boolean {
  return fiches.some(isSoon);
}
