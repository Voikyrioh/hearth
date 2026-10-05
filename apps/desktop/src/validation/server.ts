import type { MessageKey } from "@/i18n";
import { DEFAULT_PORT, type ServerInfo } from "@/link";

/**
 * Contrôles des saisies d'un serveur, pour répondre pendant la frappe. La règle de référence
 * est dans `hearth-link` (`domain/book.rs`, BR-CONN-008 et formats d'adresse) : la coquille
 * la refait à l'enregistrement, ces fonctions n'en sont que le miroir immédiat (mêmes cas, voir
 * `validation/server.test.ts`).
 */

/** Longueur maximale du nom d'un serveur. */
export const NAME_MAX_CHARS = 255;

const IPV4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/;
const LABEL = /^[A-Za-z0-9_](?:[A-Za-z0-9_-]{0,61}[A-Za-z0-9_])?$/;

function isIPv4(text: string): boolean {
  const match = IPV4.exec(text);
  return match?.slice(1).every((part) => Number(part) <= 255) === true;
}

/** IPv6 : groupes hexadécimaux séparés par `:`, au plus un `::` ; une IPv4 peut finir l'adresse. */
function isIPv6(text: string): boolean {
  if (!text.includes(":") || /[^0-9A-Fa-f:.]/.test(text)) return false;
  const halves = text.split("::");
  if (halves.length > 2) return false;
  const groups = (part: string) => (part === "" ? [] : part.split(":"));
  const all = halves.flatMap(groups);
  let count = 0;
  for (const [index, group] of all.entries()) {
    if (index === all.length - 1 && group.includes(".")) {
      if (!isIPv4(group)) return false;
      count += 2;
    } else if (/^[0-9A-Fa-f]{1,4}$/.test(group)) {
      count += 1;
    } else {
      return false;
    }
  }
  return halves.length === 2 ? count < 8 : count === 8;
}

/** Adresse valide : IPv4, IPv6 (avec ou sans crochets) ou nom (un mot, ou un nom pointé). */
export function isValidHost(host: string): boolean {
  if (host === "" || host.length > 253) return false;
  const bare = host.startsWith("[") && host.endsWith("]") ? host.slice(1, -1) : host;
  if (isIPv6(bare)) return true;
  if (/^[\d.]+$/.test(host)) return isIPv4(host);
  return host
    .replace(/\.$/, "")
    .split(".")
    .every((label) => LABEL.test(label));
}

export type NameError = Extract<MessageKey, "validation.nameRequired" | "validation.nameTaken">;

/** Erreur du nom (texte de `fr.ts`), ou `null`. `others` : les autres serveurs du carnet. */
export function nameError(name: string, others: readonly ServerInfo[]): MessageKey | null {
  const trimmed = name.trim();
  if (trimmed === "" || [...trimmed].length > NAME_MAX_CHARS) return "validation.nameRequired";
  const lowered = trimmed.toLowerCase();
  if (others.some((server) => server.name.trim().toLowerCase() === lowered)) {
    return "validation.nameTaken";
  }
  return null;
}

export function hostError(host: string): MessageKey | null {
  const trimmed = host.trim();
  if (trimmed === "") return "validation.hostRequired";
  return isValidHost(trimmed) ? null : "validation.hostInvalid";
}

/** Port saisi : vide = port par défaut (`null`) ; sinon un entier de 1 à 65535. `undefined` si invalide. */
export function parsePort(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (!/^\d{1,5}$/.test(trimmed)) return undefined;
  const port = Number(trimmed);
  return port >= 1 && port <= 65535 ? port : undefined;
}

export function portError(text: string): MessageKey | null {
  return parsePort(text) === undefined ? "validation.portInvalid" : null;
}

/** Le serveur du carnet qui a déjà cette adresse et ce port, s'il y en a un. */
export function serverAt(
  servers: readonly ServerInfo[],
  host: string,
  port: number | null,
): ServerInfo | undefined {
  const wanted = port ?? DEFAULT_PORT;
  return servers.find(
    (server) => server.host.toLowerCase() === host.trim().toLowerCase() && server.port === wanted,
  );
}
