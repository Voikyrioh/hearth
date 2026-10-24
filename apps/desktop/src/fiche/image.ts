/**
 * L'image d'une fiche telle que l'écran la dit (HRT-102, BR-FICHE-009, BR-FICHE-025), des fonctions pures sans E/S : le
 * registre d'où le serveur la récupère, déduit du dépôt. Aucune règle de jeu.
 */

/**
 * Le registre d'un dépôt : le premier élément du chemin quand il ressemble à un nom de machine (il contient un point ou
 * un deux-points, ou c'est `localhost`), sinon `null` : le registre public par défaut.
 */
export function registryOf(repository: string): string | null {
  const slash = repository.indexOf("/");
  if (slash < 0) return null;
  const first = repository.slice(0, slash);
  return first.includes(".") || first.includes(":") || first === "localhost" ? first : null;
}
