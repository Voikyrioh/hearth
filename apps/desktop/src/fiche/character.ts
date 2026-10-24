import { type MessageKey, t } from "@/i18n";
import type { fr } from "@/i18n/fr";

/**
 * Dire un caractère refusé en mots, et proposer le texte corrigé (revue UX 2026-10-24, constat 5). Une espace ne se
 * voit pas : « un caractère que ce jeu ne sait pas lire : » sans rien derrière ne dit pas quoi corriger. Des fonctions
 * pures, sans E/S ; la correction ne vient que d'un texte non secret, jamais d'un mot de passe.
 */

/** Caractère visible ou espace courante → clé de `fiche.character.names` (les textes sont dans `fr.ts`). */
const KEYS: Record<string, keyof typeof fr.fiche.character.names> = {
  " ": "space",
  "\t": "tab",
  "\u00A0": "nbsp",
  "!": "exclamation",
  "?": "question",
  ".": "period",
  ",": "comma",
  ";": "semicolon",
  ":": "colon",
  "'": "apostrophe",
  "\u2019": "apostrophe",
  '"': "quote",
  "«": "guillemetOpen",
  "»": "guillemetClose",
  "-": "hyphen",
  _: "underscore",
  "/": "slash",
  "\\": "backslash",
  "|": "pipe",
  "(": "parenOpen",
  ")": "parenClose",
  "[": "bracketOpen",
  "]": "bracketClose",
  "{": "braceOpen",
  "}": "braceClose",
  "<": "less",
  ">": "greater",
  "@": "at",
  "#": "hash",
  "&": "ampersand",
  "*": "asterisk",
  "+": "plus",
  "=": "equals",
  "%": "percent",
  $: "dollar",
  "~": "tilde",
  "^": "caret",
  "`": "backtick",
};

/** Un caractère qu'on ne voit pas (contrôle, format, séparateur) : il se nomme par son code. */
const INVISIBLE = /^[\p{Cc}\p{Cf}\p{Z}]$/u;

/** Le caractère dit en mots (« l'espace », « le point d'exclamation »), ou par son code s'il est invisible. */
export function characterName(character: string): string {
  const key = KEYS[character];
  if (key !== undefined) return t(`fiche.character.names.${key}` as MessageKey);
  if (INVISIBLE.test(character)) {
    const code = (character.codePointAt(0) ?? 0).toString(16).toUpperCase().padStart(4, "0");
    return t("fiche.character.invisible", { code: `U+${code}` });
  }
  if (/\p{L}/u.test(character) && character.normalize("NFD") !== character) {
    return t("fiche.character.accented", { character });
  }
  return t("fiche.character.other", { character });
}

const ALLOWED: Record<string, RegExp> = {
  file_stem: /[A-Za-z0-9_-]/,
  arg_safe: /[A-Za-z0-9._:-]/,
};

/**
 * Le texte corrigé : les accents retirés, les espaces remplacées par un souligné, les autres caractères refusés
 * enlevés. `null` quand le jeu de caractères n'est pas connu (rien à proposer), quand rien ne change ou qu'il ne reste
 * rien.
 */
export function correctedText(charset: string | null, value: string): string | null {
  const allowed = charset === null ? undefined : ALLOWED[charset];
  if (!allowed) return null;
  let out = "";
  for (const char of value.normalize("NFD").replace(/\p{M}+/gu, "")) {
    if (allowed.test(char)) out += char;
    else if (/\s/u.test(char) && !out.endsWith("_")) out += "_";
  }
  if (charset === "arg_safe") out = out.replace(/^[-+]+/, "");
  return out === "" || out === value ? null : out;
}
