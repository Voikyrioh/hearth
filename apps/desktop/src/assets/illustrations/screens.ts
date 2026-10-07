import type { IllustrationName } from "./index";

// Quelle illustration pour quel écran vide : LE fichier à changer pour en ajouter, en retirer ou en
// échanger une (`null` : aucune). Le nom doit exister dans `ILLUSTRATIONS` (`index.ts`). Les pages
// la lisent, et leurs tests (Vitest et Playwright) aussi : rien d'autre à toucher, sauf pour un
// NOUVEAU dessin : sa source et son entrée dans `scripts/illustration-spec.mjs`, puis sa ligne dans
// `index.ts` et dans `ILLUSTRATION_FILES` ci-dessous.
export const SCREEN_ILLUSTRATIONS: {
  welcome: IllustrationName | null;
  journal: IllustrationName | null;
  offline: IllustrationName | null;
  accounts: IllustrationName | null;
} = {
  welcome: "firstLaunch",
  journal: "journal",
  offline: "offline",
  accounts: null,
};

/** Fragment du nom de fichier servi, par illustration (les tests de bout en bout ne chargent pas les SVG). */
export const ILLUSTRATION_FILES: Record<IllustrationName, string> = {
  firstLaunch: "vide-premier-lancement",
  journal: "vide-journal",
  offline: "vide-hors-ligne",
};

/** Fragment du fichier attendu pour un écran ; échoue si l'écran n'a pas d'illustration. */
export function expectedFile(screen: keyof typeof SCREEN_ILLUSTRATIONS): string {
  const name = SCREEN_ILLUSTRATIONS[screen];
  if (name === null) throw new Error(`l'écran « ${screen} » n'a pas d'illustration`);
  return ILLUSTRATION_FILES[name];
}
