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
/** Combien de caractères au-delà de la coupe on regarde pour voir où finit un caractère composé. */
const CLUSTER_LOOKAHEAD = 32;
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

const EXTENDS = /^(?:\p{M}|‍|️|[\u{1F3FB}-\u{1F3FF}]|[\u{E0020}-\u{E007F}])/u;
const HIGH = (code: number) => code >= 0xd800 && code <= 0xdbff;

/**
 * Le début de `text`, d'au plus `max` unités UTF-16, coupé sur une FRONTIÈRE DE CARACTÈRE : jamais au milieu d'une
 * paire de substitution (un émoji), d'un caractère composé (lettre et accent) ni d'un émoji à plusieurs éléments
 * (famille, drapeau, teinte de peau). Un caractère qui dépasse la coupe est retiré en entier.
 */
export function cutOnCharacter(text: string, max: number): string {
  if (text.length <= max) return text;
  if (typeof Intl !== "undefined" && "Segmenter" in Intl) {
    let end = 0;
    const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
    for (const part of segmenter.segment(text.slice(0, max + CLUSTER_LOOKAHEAD))) {
      const stop = part.index + part.segment.length;
      if (stop > max) break;
      end = stop;
    }
    return text.slice(0, end);
  }
  // Sans découpeur : jamais au milieu d'une paire, ni juste avant une marque qui se colle au caractère précédent.
  let end = max;
  if (HIGH(text.charCodeAt(end - 1))) end -= 1;
  while (end > 0 && EXTENDS.test(text.slice(end, end + 2))) {
    end -= 1;
    if (HIGH(text.charCodeAt(end - 1))) end -= 1;
  }
  return text.slice(0, end);
}

/** Le nombre d'unités UTF-16 qui portent les `count` premiers points de code de `text`. */
function unitsOf(text: string, count: number): number {
  let units = 0;
  for (let n = 0; n < count && units < text.length; n += 1) {
    units += (text.codePointAt(units) ?? 0) > 0xffff ? 2 : 1;
  }
  return units;
}

/** Le nombre de caractères d'un texte tel qu'on le lit (un émoji, un caractère composé : un seul). */
export function charCount(text: string): number {
  if (typeof Intl !== "undefined" && "Segmenter" in Intl) {
    let n = 0;
    const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });
    for (const _ of segmenter.segment(text)) n += 1;
    return n;
  }
  return Array.from(text).length;
}

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
  // Le plafond de l'agent compte des caractères (points de code) ; la coupe tombe sur une frontière de caractère.
  if (text.length > MAX_LINE_CHARS) {
    text = cutOnCharacter(text, unitsOf(text, MAX_LINE_CHARS));
  }
  return safeText(text.split("\t").join(TAB));
}

/** Le début montré d'une ligne repliée. */
export function foldedText(text: string): string {
  return cutOnCharacter(text, FOLD_CHARS);
}
