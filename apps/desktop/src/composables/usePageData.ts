import { pageSlot } from "./pageSlot";

// FIX:01M4E5D4JY66T0ETEMRY1DZQZK (C46) : une page qui n'a encore RIEN chargé le dit au gabarit du serveur, qui n'affiche alors pas
// d'estampille « Vu il y a… » (rien n'a été vu). Le gabarit appelle `providePageData`, chaque page qui peut être
// vide faute de lien appelle `usePageData` ; une page qui ne l'appelle pas est réputée avoir ses données.
const slot = pageSlot<boolean>("page-data", true);

/** Gabarit : l'état « la page affichée a des données », vrai par défaut. */
export const providePageData = slot.provide;

/** Page : dit si elle a de quoi afficher (une liste lue, un état lu). Remis à vrai quand la page part. */
export const usePageData = slot.use;
