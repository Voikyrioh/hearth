// Ce que `build-illustrations.mjs` produit, en données (lu aussi par `src/assets/identity-sync.test.ts`).
// HRT-31 : les trois illustrations d'écran vide choisies par le détenteur (Q20), images générées
// dont le fond est rendu transparent. `display` : largeur d'affichage en px CSS (jetons
// `--illustration-lg` 280 et `--illustration-md` 220) ; la sortie fait au plus deux fois cette taille.
export const ILLUSTRATIONS = [
  {
    name: "vide-premier-lancement",
    source: "vide-premier-lancement-gemini-v1.png",
    display: 280,
  },
  { name: "vide-journal", source: "vide-journal-gemini-v1.png", display: 220 },
  { name: "vide-hors-ligne", source: "vide-hors-ligne-gemini-v2.png", display: 220 },
];
/** Facteur d'échelle de sortie par rapport à la taille d'affichage (écrans 200 %). */
export const OUTPUT_SCALE = 2;
/**
 * Seuil bas de l'alpha (part de l'écart au fond sous laquelle un pixel est du fond) : absorbe le
 * bruit de compression des sources (écart de 3 à 4 niveaux autour du fond, mesuré). Au-dessus, la
 * rampe est continue : les lueurs et les dégradés gardent leur transparence partielle.
 */
export const ALPHA_FLOOR = 0.02;
// Version du rendu : à monter À LA MAIN quand `build-illustrations.mjs` change ce qu'il produit.
export const RENDER_VERSION = 1;
