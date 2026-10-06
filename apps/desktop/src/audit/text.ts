/**
 * Valeurs du journal rendues inoffensives. Tout ce que montre le journal (identifiants tentés, noms
 * de poste, cibles, raisons) est du texte saisi par des tiers : il ne passe jamais par `v-html`, et
 * ses caractères de contrôle ne doivent ni casser la mise en page ni se faire passer pour autre
 * chose (retour à la ligne, retour arrière, inversion du sens de lecture).
 */

/** Marque affichée à la place d'un caractère de contrôle (point médian). */
export const CONTROL_MARK = String.fromCharCode(0xb7);

/** Ellipse des valeurs tronquées. */
const ELLIPSIS = String.fromCharCode(0x2026);

/** Longueur au-delà de laquelle une valeur est tronquée dans une cellule du tableau. */
export const CELL_MAX_CHARS = 80;

// Plages de points de code rendues inoffensives : C0 (tabulation et sauts de ligne compris), DEL,
// C1, trait d'union conditionnel, marques et isolats bidirectionnels, séparateurs de ligne et de
// paragraphe, caractères invisibles de formatage.
const RANGES: readonly (readonly [number, number])[] = [
  [0x00, 0x1f],
  [0x7f, 0x9f],
  [0xad, 0xad],
  [0x61c, 0x61c],
  [0x180e, 0x180e],
  [0x200b, 0x200f],
  [0x2028, 0x202e],
  [0x2060, 0x206f],
  [0xfeff, 0xfeff],
  [0xfff9, 0xfffb],
];

const hex = (code: number) => `\\${"u"}${code.toString(16).padStart(4, "0")}`;
const CONTROL = new RegExp(`[${RANGES.map(([a, b]) => `${hex(a)}-${hex(b)}`).join("")}]`, "g");

/** Le texte sans caractère de contrôle : chacun est remplacé par une marque visible. */
export function safeText(value: string | null | undefined): string {
  return (value ?? "").replace(CONTROL, CONTROL_MARK);
}

/**
 * Le texte tronqué à `max` caractères (par points de code : jamais au milieu d'une paire de
 * substitution), avec une ellipse. `truncated` dit s'il y a quelque chose à aller voir en entier.
 */
export function clip(value: string | null | undefined, max = CELL_MAX_CHARS) {
  const text = safeText(value);
  const chars = Array.from(text);
  if (chars.length <= max) return { text, truncated: false };
  return { text: `${chars.slice(0, max - 1).join("")}${ELLIPSIS}`, truncated: true };
}
