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
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
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
