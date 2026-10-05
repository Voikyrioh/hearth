import * as Vue from "vue";
import { ErrorCodes } from "vue";

// Vue donne à `onErrorCaptured` un `info` qui dépend du build : un libellé lisible en
// développement (« render function »), une adresse de référence en production
// (« https://vuejs.org/error-reference/#runtime-1 », le code à la fin). On trie donc sur les
// CODES exportés par Vue (`ErrorCodes`), jamais sur des libellés écrits ici.
const PROD_PREFIX = "https://vuejs.org/error-reference/#runtime-";

// Codes de rendu : setup, rendu, mise à jour de composant, plus les crochets de cycle de vie
// (codes courts de Vue : avant/après création, montage, mise à jour, démontage, activation).
const RENDER_CODES = new Set<string>([
  String(ErrorCodes.SETUP_FUNCTION),
  String(ErrorCodes.RENDER_FUNCTION),
  String(ErrorCodes.COMPONENT_UPDATE),
  "bc",
  "c",
  "bm",
  "m",
  "bu",
  "u",
  "bum",
  "um",
  "da",
  "a",
]);

type Labels = Record<string | number, string | undefined>;

// `ErrorTypeStrings` (libellés de développement) existe à l'exécution mais pas dans les types publics.
const VUE_LABELS = (Vue as unknown as { ErrorTypeStrings?: Labels }).ErrorTypeStrings ?? {};

/**
 * Vrai si `info` désigne une erreur de RENDU (ou de cycle de vie) d'un composant : seule
 * celle-là remplace une page par le repli. Gère les deux formes (libellé en développement,
 * adresse avec code en production).
 */
export function isRenderError(info: string, devLabels: Labels = VUE_LABELS): boolean {
  if (info.startsWith(PROD_PREFIX)) return RENDER_CODES.has(info.slice(PROD_PREFIX.length));
  for (const code of RENDER_CODES) {
    const label = devLabels[code];
    if (label !== undefined && label === info) return true;
  }
  return false;
}
