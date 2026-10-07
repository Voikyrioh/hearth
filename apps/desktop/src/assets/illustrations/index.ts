// Illustrations des écrans vides : images générées choisies par le détenteur (ADR-0027, Q20), fond
// transparent, produites par `npm run build:illustrations` depuis `source/` et servies comme
// fichiers (CSP `img-src 'self'`). Pour en changer une : remplacer sa source, ajuster
// `scripts/illustration-spec.mjs`, relancer le script.
import offline from "./vide-hors-ligne.png";
import journal from "./vide-journal.png";
import firstLaunch from "./vide-premier-lancement.png";

export const ILLUSTRATIONS = { firstLaunch, journal, offline } as const;

export type IllustrationName = keyof typeof ILLUSTRATIONS;
