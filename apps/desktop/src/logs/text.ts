import { safeText } from "@/audit/text";

/**
 * Le texte d'une ligne de journal rendu inoffensif (HRT-67, BR-SLOG-013, BR-SLOG-019). L'agent nettoie déjà chaque
 * ligne ; ce module est la SECONDE garde, côté client : un agent plus ancien, plus récent ou compromis ne doit rien
 * faire exécuter ni faire passer pour autre chose. Rien d'une ligne n'est jamais interprété : pas de HTML (le texte est
 * interpolé), pas de lien fabriqué, pas de séquence de terminal.
 */

/** Longueur au-delà de laquelle le texte reçu est coupé, quoi qu'en dise l'agent (le plafond de l'agent). */
export const MAX_LINE_CHARS = 10_000;
/** Au-delà, la ligne est REPLIÉE : ce début est montré, la suite s'affiche à la demande. */
export const FOLD_CHARS = 600;
/** Largeur d'une tabulation affichée (espaces). */
const TAB = "    ";

const ESC = String.fromCharCode(0x1b);
const BEL = String.fromCharCode(0x07);
const CSI8 = String.fromCharCode(0x9b);
const ST8 = String.fromCharCode(0x9c);
const OSC8 = String.fromCharCode(0x9d);

// Séquences de terminal. Chaque motif est borné (jamais de retour sur trace catastrophique) et les chaînes de
// commande (titre de fenêtre, lien, DCS...) sont retirées JUSQU'À leur fin, texte compris : « ESC ] 0 ; titre BEL »
// ne laisse pas « titre » dans la ligne.
const SEQUENCES = [
  // Chaînes de commande terminées par BEL ou ST (ESC \) : OSC (]), DCS (P), SOS (X), PM (^), APC (_).
  new RegExp(`${ESC}[\\]PX^_][^${BEL}${ESC}]{0,4096}(?:${BEL}|${ESC}\\\\|$)`, "g"),
  new RegExp(`${OSC8}[^${BEL}${ESC}${ST8}]{0,4096}(?:${BEL}|${ST8}|$)`, "g"),
  // CSI : ESC [ paramètres intermédiaires final (et sa forme sur un octet).
  new RegExp(`(?:${ESC}\\[|${CSI8})[0-?]{0,64}[ -/]{0,16}[@-~]?`, "g"),
  // Les autres séquences d'échappement : ESC, intermédiaires, un caractère final.
  new RegExp(`${ESC}[ -/]{0,16}[0-~]?`, "g"),
];

/** Le texte sans séquence de terminal. */
export function stripTerminal(raw: string): string {
  let text = raw;
  for (const pattern of SEQUENCES) text = text.replace(pattern, "");
  return text;
}

/**
 * Le texte à afficher : séquences de terminal retirées, tabulations en espaces, tout caractère de contrôle,
 * bidirectionnel ou invisible remplacé par une marque visible (`safeText`), coupé à `MAX_LINE_CHARS`. Jamais de HTML :
 * l'appelant l'interpole.
 */
export function displayText(raw: string | null | undefined): string {
  let text = stripTerminal(raw ?? "");
  if (text.length > MAX_LINE_CHARS) {
    // Par points de code : jamais au milieu d'une paire de substitution.
    text = Array.from(text.slice(0, MAX_LINE_CHARS + 1))
      .slice(0, MAX_LINE_CHARS)
      .join("");
  }
  return safeText(text.split("\t").join(TAB));
}

/** Le début montré d'une ligne repliée. */
export function foldedText(text: string): string {
  if (text.length <= FOLD_CHARS) return text;
  return Array.from(text.slice(0, FOLD_CHARS + 1))
    .slice(0, FOLD_CHARS)
    .join("");
}
