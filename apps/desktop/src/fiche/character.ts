import { t } from "@/i18n";

/**
 * Dire un caractère refusé en mots, et proposer le texte corrigé (revue UX 2026-10-24, constat 5). Une espace ne se
 * voit pas : « un caractère que ce jeu ne sait pas lire : » sans rien derrière ne dit pas quoi corriger. Des fonctions
 * pures, sans E/S ; la correction ne vient que d'un texte non secret, jamais d'un mot de passe.
 */

const NAMES: Record<string, string> = {
  " ": "l'espace",
  "\t": "la tabulation",
  "\u00A0": "l'espace insécable",
  "!": "le point d'exclamation",
  "?": "le point d'interrogation",
  ".": "le point",
  ",": "la virgule",
  ";": "le point-virgule",
  ":": "les deux-points",
  "'": "l'apostrophe",
  "\u2019": "l'apostrophe",
  '"': "le guillemet",
  "«": "le guillemet ouvrant",
  "»": "le guillemet fermant",
  "-": "le tiret",
  _: "le souligné",
  "/": "la barre oblique",
  "\\": "la barre oblique inverse",
  "|": "la barre verticale",
  "(": "la parenthèse ouvrante",
  ")": "la parenthèse fermante",
  "[": "le crochet ouvrant",
  "]": "le crochet fermant",
  "{": "l'accolade ouvrante",
  "}": "l'accolade fermante",
  "<": "le signe inférieur",
  ">": "le signe supérieur",
  "@": "l'arobase",
  "#": "le dièse",
  "&": "l'esperluette",
  "*": "l'astérisque",
  "+": "le signe plus",
  "=": "le signe égal",
  "%": "le signe pourcentage",
  $: "le dollar",
  "~": "le tilde",
  "^": "l'accent circonflexe",
  "`": "l'accent grave",
};

/** Le caractère dit en mots (« l'espace », « le point d'exclamation »), ou la lettre entre guillemets si elle n'a pas de nom. */
export function characterName(character: string): string {
  const known = NAMES[character];
  if (known !== undefined) return known;
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
