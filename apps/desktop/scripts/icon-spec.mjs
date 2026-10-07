// Ce que `build-icons.mjs` produit, en données (lu aussi par `src/assets/identity.test.ts`).
export const TRAY_STATES = [
  "aucun-serveur",
  "connecte",
  "reconnexion",
  "hors-ligne",
  "session-expiree",
  "acces-revoque",
];
export const TRAY_SIZES = [16, 20, 24, 32];
// Dessin dédié (grille de 16) pour les petites tailles, logo détaillé au-dessus.
export const ICO_FRAMES = [
  { size: 16, svg: "app-icon-16.svg" },
  { size: 24, svg: "app-icon-16.svg" },
  { size: 32, svg: "app-icon-16.svg" },
  { size: 48, svg: "app-icon.svg" },
  { size: 64, svg: "app-icon.svg" },
  { size: 256, svg: "app-icon.svg" },
];
