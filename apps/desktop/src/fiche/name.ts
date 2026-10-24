import { nameIssue } from "@/deploy/rules";

/**
 * Le nom du service proposé par la fiche (HRT-102, BR-FICHE-015), des fonctions pures sans E/S : le nom du jeu en
 * minuscules, sans accent, réduit aux lettres, chiffres, tirets et points que l'agent accepte ; s'il est pris, le même
 * nom suivi de « -2 », « -3 »… jusqu'au premier libre. Les règles du nom sont celles d'un déploiement (BR-DEPLOY-003).
 */

/** Le nom du jeu réduit à ce qu'un nom de service peut contenir ; `jeu` s'il n'en reste rien. */
export function slugOf(gameName: string): string {
  const folded = gameName
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9.-]+/g, "-")
    .replace(/^[^a-z0-9]+/, "")
    .replace(/[^a-z0-9]+$/, "")
    .slice(0, 55);
  return folded === "" ? "jeu" : folded;
}

/** Le premier nom libre : `terraria`, puis `terraria-2`, `terraria-3`… (sans tenir compte de la casse). */
export function proposedName(gameName: string, taken: readonly string[]): string {
  const used = new Set(taken.map((name) => name.toLowerCase()));
  const base = slugOf(gameName);
  if (!used.has(base) && nameIssue(base) === null) return base;
  for (let n = 2; n < 1000; n += 1) {
    const candidate = `${base}-${n}`;
    if (!used.has(candidate) && nameIssue(candidate) === null) return candidate;
  }
  return base;
}
