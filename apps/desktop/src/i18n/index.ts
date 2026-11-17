import type { AppError } from "@/bindings";
import { fr, type MessageKey } from "./fr";

export type { MessageKey } from "./fr";

/** Texte français pour une clé ; `{nom}` est remplacé par `params.nom`. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  let node: unknown = fr;
  for (const part of key.split(".")) {
    node = (node as Record<string, unknown>)[part];
  }
  const text = node as string;
  if (!params) return text;
  const filled = text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
  return typeof params.n === "number" ? agreed(filled, params.n) : filled;
}

/**
 * Les accords d'un compte écrits « mot(s) » : « 1 réglage différent », « 3 réglages différents » (revue UX du 17/11 : jamais
 * de parenthèses dans une phrase). Seuls `(s)`, `(x)` et `(e)` sont lus, et seulement quand le texte porte un nombre `n`.
 */
export function agreed(text: string, n: number): string {
  const one = n === 1 || n === 0;
  return text.replace(/\((s|x|e)\)/g, (_match, mark: string) =>
    one ? "" : mark === "e" ? "" : mark,
  );
}

// Exhaustif : une nouvelle `kind` côté Rust ne compile pas ici sans son texte.
const ERROR_KEYS: Record<AppError["kind"], MessageKey> = {
  store: "errors.store",
  autostart: "errors.autostart",
  logs: "errors.logs",
};

/** Texte à montrer pour une erreur typée du cœur Rust, d'après son `kind`. */
export function errorKey(kind: AppError["kind"]): MessageKey {
  return ERROR_KEYS[kind];
}
