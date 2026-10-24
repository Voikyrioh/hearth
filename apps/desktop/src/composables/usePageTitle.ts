import { pageSlot } from "./pageSlot";

// Un titre d'en-tête que seule la page connaît (revue UX 2026-10-24, constat 17) : « Terraria » pour la fiche d'un jeu,
// « Installer Terraria » pour son installation, à chaque étape. Une page qui n'appelle pas `usePageTitle` garde le titre
// de sa route (`meta.title`). Tant que la page n'a pas lu son titre (`null`), l'en-tête garde celui de la route :
// jamais un en-tête vide.
const slot = pageSlot<string | null>("page-title", null);

/** Gabarit : le titre posé par la page, `null` par défaut. */
export const providePageTitle = slot.provide;

/** Page : dit son titre (`null` : pas encore). Remis à `null` quand la page part. */
export const usePageTitle = slot.use;
