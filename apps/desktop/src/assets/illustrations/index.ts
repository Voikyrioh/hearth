// Illustrations des écrans vides : SVG à plat (palette de la bible, fond transparent) servis comme
// fichiers (CSP `img-src 'self'`). Pour en changer une, remplacer le fichier : aucun code à toucher.
import offline from "./vide-hors-ligne.svg";
import journal from "./vide-journal.svg";
import firstLaunch from "./vide-premier-lancement.svg";

export const ILLUSTRATIONS = { firstLaunch, journal, offline } as const;

export type IllustrationName = keyof typeof ILLUSTRATIONS;
