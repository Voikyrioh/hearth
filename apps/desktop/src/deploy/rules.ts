/**
 * Les règles de saisie d'un déploiement (HRT-69, `BR-DEPLOY-002` à `BR-DEPLOY-007`, `BR-DEPLOY-014`), des fonctions
 * pures sans E/S. C'est le MIROIR de ce que l'agent juge (`crates/hearth-agent/src/domain/services/{deploy,command}.rs`) :
 * le formulaire dit d'avance, honnêtement, ce que le serveur refusera. Le serveur reste l'arbitre à chaque demande
 * (`BR-DEPLOY-032`) : ce qui est calculé ici est indicatif. Chaque fonction rend un CODE, jamais un texte : les phrases
 * sont dans `deploy/messages.ts`. Aucune fonction ne cite la valeur reçue.
 *
 * Les règles de CONFLIT DE PORT (qui tient quoi sur le serveur) ne sont PAS ici : elles sont jugées par l'agent
 * (`POST /deployments/check`), le formulaire n'en recopie aucune. Ne sont ici que les règles de FORME.
 */

export const NAME_MIN_CHARS = 2;
export const NAME_MAX_CHARS = 63;
export const IMAGE_MAX_CHARS = 255;
export const TAG_MAX_CHARS = 128;
export const MAX_ENV_VARS = 100;
export const MAX_PORT_MAPPINGS = 50;
export const MAX_ENV_NAME_CHARS = 255;
export const MAX_ENV_VALUE_CHARS = 8192;
export const MAX_COMMAND_CHARS = 4096;
export const MAX_COMMAND_ARGS = 64;
export const MAX_COMMAND_ARG_CHARS = 1024;
/** Sous ce port, l'avertissement de `BR-DEPLOY-007`. */
export const PRIVILEGED_PORT_BELOW = 1024;

export type NameIssue = "required" | "too_short" | "too_long" | "first" | "chars";

/** Le nom d'un service (`BR-DEPLOY-003`) : lettres sans accent, chiffres, tirets et points, 2 à 63 caractères. */
export function nameIssue(name: string): NameIssue | null {
  if (name === "") return "required";
  const length = [...name].length;
  if (length > NAME_MAX_CHARS) return "too_long";
  if (!/^[A-Za-z0-9]/.test(name)) return /^[A-Za-z0-9.-]+$/.test(name) ? "first" : "chars";
  if (!/^[A-Za-z0-9.-]+$/.test(name)) return "chars";
  if (length < NAME_MIN_CHARS) return "too_short";
  return null;
}

/**
 * Sépare le dépôt et la version d'une image (`BR-DEPLOY-005`) : la version suit le dernier « : » situé après le
 * dernier « / » (un « : » avant est un port de registre). `embedded` : la version écrite dans le champ de l'image.
 */
export function splitImage(image: string): { repository: string; embedded: string | null } {
  const text = image.trim();
  const afterSlash = text.lastIndexOf("/") + 1;
  const colon = text.slice(afterSlash).lastIndexOf(":");
  if (colon < 0) return { repository: text, embedded: null };
  const cut = afterSlash + colon;
  const embedded = text.slice(cut + 1);
  return { repository: text.slice(0, cut), embedded: embedded === "" ? null : embedded };
}

export type ImageIssue = "required" | "digest" | "too_long" | "format";

const PATH_COMPONENT = /^[a-z0-9]+(?:(?:\.|_|__|-+)[a-z0-9]+)*$/;

function validPathComponent(component: string): boolean {
  return PATH_COMPONENT.test(component);
}

/** `registre[:port]/chemin/en/minuscules` : la grammaire des références de Docker, sans version. */
export function validRepository(repository: string): boolean {
  const parts = repository.split("/");
  let path = parts;
  const first = parts[0] ?? "";
  if (parts.length > 1 && (first.includes(".") || first.includes(":") || first === "localhost")) {
    const colon = first.indexOf(":");
    const name = colon < 0 ? first : first.slice(0, colon);
    const port = colon < 0 ? null : first.slice(colon + 1);
    const nameOk =
      name !== "" && /^[A-Za-z0-9.-]+$/.test(name) && !/^[.-]/.test(name) && !/[.-]$/.test(name);
    const portOk = port === null || /^[0-9]{1,5}$/.test(port);
    if (!nameOk || !portOk) return false;
    path = parts.slice(1);
  }
  return path.length > 0 && path.every(validPathComponent);
}

/** L'image telle qu'écrite dans son champ (version éventuellement comprise) : `BR-DEPLOY-005`. */
export function imageIssue(image: string): ImageIssue | null {
  const text = image.trim();
  if (text === "") return "required";
  if (text.includes("@")) return "digest";
  const { repository } = splitImage(text);
  if ([...repository].length > IMAGE_MAX_CHARS) return "too_long";
  return validRepository(repository) ? null : "format";
}

export type TagIssue = "format" | "too_long" | "conflict";

/** La version : `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`, vide permise (vaut « latest »). */
export function tagIssue(tag: string, image: string): TagIssue | null {
  const text = tag.trim();
  // Une empreinte (`@`) est refusée sur l'image avant toute question de version (l'agent juge l'image d'abord).
  const embedded = image.includes("@") ? null : splitImage(image).embedded;
  if (text !== "" && embedded !== null && embedded !== text) return "conflict";
  const merged = embedded ?? text;
  if (merged === "") return null;
  if ([...merged].length > TAG_MAX_CHARS) return "too_long";
  return /^[A-Za-z0-9_][A-Za-z0-9_.-]*$/.test(merged) ? null : "format";
}

/** La version effective : celle du champ de l'image d'abord, sinon celle du champ Version, sinon « latest ». */
export function effectiveTag(image: string, tag: string): string {
  return splitImage(image).embedded ?? (tag.trim() === "" ? "latest" : tag.trim());
}

export type EnvNameIssue = "required" | "format" | "duplicate" | "too_long";

/** Le nom d'une variable (`BR-DEPLOY-006`) : lettres, chiffres, soulignés, sans commencer par un chiffre. */
export function envNameIssue(name: string): EnvNameIssue | null {
  if (name === "") return "required";
  if ([...name].length > MAX_ENV_NAME_CHARS) return "too_long";
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name) ? null : "format";
}

export type EnvValueIssue = "too_long" | "multiline";

/** La valeur d'une variable : 8 192 caractères au plus, sur une seule ligne. */
export function envValueIssue(value: string): EnvValueIssue | null {
  if (/[\n\r\0]/.test(value)) return "multiline";
  return [...value].length > MAX_ENV_VALUE_CHARS ? "too_long" : null;
}

const SECRET_WORDS = ["PASSWORD", "PASS", "SECRET", "TOKEN", "KEY", "CREDENTIAL"];

/** Le nom évoque un secret : la valeur est masquée d'office (`BR-DEPLOY-006`). */
export function looksSecret(name: string): boolean {
  const upper = name.toUpperCase();
  return SECRET_WORDS.some((word) => upper.includes(word));
}

export type PortIssue = "required" | "integer" | "range";

/** Un numéro de port saisi : entier de 1 à 65535 (`BR-DEPLOY-007`). Rend le nombre, ou le code du défaut. */
export function parsePort(text: string): { port: number } | { issue: PortIssue } {
  const value = text.trim();
  if (value === "") return { issue: "required" };
  if (!/^[0-9]+$/.test(value)) return { issue: "integer" };
  const port = Number(value);
  if (!Number.isSafeInteger(port) || port < 1 || port > 65535) return { issue: "range" };
  return { port };
}

/**
 * `char::is_whitespace` de Rust (propriété White_Space d'Unicode) : `\s` de JavaScript y ajoute U+FEFF, que l'agent ne
 * tient pas pour une espace.
 */
function isWhitespace(char: string): boolean {
  const code = char.codePointAt(0) ?? 0;
  return (
    (code >= 0x09 && code <= 0x0d) ||
    code === 0x20 ||
    code === 0x85 ||
    code === 0xa0 ||
    code === 0x1680 ||
    (code >= 0x2000 && code <= 0x200a) ||
    code === 0x2028 ||
    code === 0x2029 ||
    code === 0x202f ||
    code === 0x205f ||
    code === 0x3000
  );
}

/** Un caractère de contrôle (Cc) autre que la tabulation et les retours à la ligne : l'agent le refuse. */
function hasControl(text: string): boolean {
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const control = code <= 0x1f || (code >= 0x7f && code <= 0x9f);
    if (control && code !== 0x09 && code !== 0x0a && code !== 0x0d) return true;
  }
  return false;
}

export type CommandIssue = "too_long" | "quote" | "control" | "args";

/**
 * Découpe une ligne de commande en arguments, SANS interpréteur (`BR-DEPLOY-014`) : des espaces séparent, une paire de
 * guillemets simples ou doubles regroupe, des morceaux accolés forment un seul argument, `\` n'a aucun sens. Le miroir
 * de `split_command` de l'agent. Une ligne vide ou faite d'espaces donne une liste vide (commande de l'image).
 */
export function splitCommand(line: string): { args: string[] } | { issue: CommandIssue } {
  if ([...line].length > MAX_COMMAND_CHARS) return { issue: "too_long" };
  if (hasControl(line)) return { issue: "control" };
  const args: string[] = [];
  let current = "";
  let started = false;
  let quote: string | null = null;
  for (const char of line) {
    if (quote !== null) {
      if (char === quote) quote = null;
      else current += char;
      continue;
    }
    if (char === "'" || char === '"') {
      quote = char;
      started = true;
    } else if (isWhitespace(char)) {
      if (started) {
        args.push(current);
        current = "";
        started = false;
      }
    } else {
      current += char;
      started = true;
    }
  }
  if (quote !== null) return { issue: "quote" };
  if (started) args.push(current);
  if (
    args.length > MAX_COMMAND_ARGS ||
    args.some((arg) => [...arg].length > MAX_COMMAND_ARG_CHARS)
  ) {
    return { issue: "args" };
  }
  return { args };
}

/** Le défaut de la commande de lancement, `null` si elle est valable ou absente. */
export function commandIssue(line: string): CommandIssue | null {
  if (line.trim() === "") return null;
  const result = splitCommand(line);
  return "issue" in result ? result.issue : null;
}
