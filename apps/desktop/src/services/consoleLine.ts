import type { ConsoleView } from "@/link";

/**
 * Une ligne de console, sa forme et sa classe (HRT-269, `BR-CONSOLE-003`, `004`, `005`) : des fonctions pures, le MIROIR
 * de `hearth_proto::console` (`normalize`, `classify`) que l'agent applique lui-même. L'agent ne croit jamais le client :
 * il normalise, reclasse et refuse (`CONSOLE_CLASS_MISMATCH`) ; ceci sert à dire d'avance pourquoi « Envoyer » est
 * inactif et si le mot de passe sera demandé. La politique des verbes vient de la FICHE du jeu (jamais écrite ici).
 */

/** Au plus ce nombre de points de code dans une ligne. */
export const MAX_LINE_CHARS = 256;

export type LineError = "empty" | "too_long" | "control_char";

/** Un caractère qui n'a rien à faire dans une commande : contrôle, séparateur de ligne, invisible, renversement du sens. */
export function isRiskyChar(char: string): boolean {
  const code = char.codePointAt(0) ?? 0;
  return (
    code <= 0x1f ||
    (code >= 0x7f && code <= 0x9f) ||
    code === 0x2028 ||
    code === 0x2029 ||
    code === 0xfeff ||
    (code >= 0x200b && code <= 0x200f) ||
    (code >= 0x202a && code <= 0x202e) ||
    (code >= 0x2060 && code <= 0x2064) ||
    (code >= 0x2066 && code <= 0x2069)
  );
}

export type Normalized = { ok: true; line: string } | { ok: false; error: LineError };

/** Retire les espaces de tête et de fin et UNE barre oblique de tête ; refuse vide, trop long, caractère à risque. */
export function normalizeLine(raw: string, maxChars: number = MAX_LINE_CHARS): Normalized {
  for (const char of raw) if (isRiskyChar(char)) return { ok: false, error: "control_char" };
  const trimmed = raw.trim();
  const line = (trimmed.startsWith("/") ? trimmed.slice(1) : trimmed).trimStart();
  if (line.length === 0) return { ok: false, error: "empty" };
  if ([...line].length > maxChars) return { ok: false, error: "too_long" };
  return { ok: true, line };
}

/** Le verbe jugé : le premier mot en minuscules ASCII, sans le préfixe `minecraft:`. */
export function verbOf(line: string): string {
  const first = line.split(" ")[0] ?? "";
  const lower = first.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  return lower.startsWith("minecraft:") ? lower.slice("minecraft:".length) : lower;
}

/** Le deuxième mot, pris pour `{joueur}` quand il a la forme d'un pseudo (BR-CONSOLE-009). */
export function playerOf(line: string): string | null {
  const arg = line.split(" ").filter((word) => word.length > 0)[1];
  return arg !== undefined && /^[A-Za-z0-9_]{1,16}$/.test(arg) ? arg : null;
}

export type LineClass =
  | { kind: "free" }
  | { kind: "sensitive"; verb: string; title: string; sentence: string; player: string | null }
  | { kind: "refused"; verb: string; reason: string };

/** La classe d'une ligne normalisée selon la politique de la fiche. */
export function classifyLine(policy: ConsoleView["policy"], line: string): LineClass {
  const verb = verbOf(line);
  const refused = policy.refused.find((item) => item.verb === verb);
  if (refused) return { kind: "refused", verb, reason: refused.reason };
  const sensitive = policy.sensitive.find((item) => item.verb === verb);
  if (sensitive) {
    const player = sensitive.target === "first_arg" ? playerOf(line) : null;
    return {
      kind: "sensitive",
      verb,
      title: sensitive.title,
      sentence: sensitive.sentence,
      player,
    };
  }
  return { kind: "free" };
}

/** Le titre d'une confirmation : `{joueur}` est remplacé par le pseudo quand il a la forme d'un pseudo, sinon retiré. */
export function sensitiveTitle(title: string, player: string | null): string {
  if (player !== null) return title.replaceAll("{joueur}", player);
  return title
    .replaceAll(" de {joueur}", "")
    .replaceAll(" à {joueur}", "")
    .replaceAll(" {joueur}", "")
    .replaceAll("{joueur}", "")
    .replace(/\s+([?!.])/g, "$1");
}

/** Le code signé de la classe : `1` libre, `2` sensible ; une commande refusée n'a pas d'acte. */
export function signedClass(classOf: LineClass): 1 | 2 | null {
  if (classOf.kind === "free") return 1;
  if (classOf.kind === "sensitive") return 2;
  return null;
}
