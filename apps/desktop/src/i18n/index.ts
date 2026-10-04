import { fr, type MessageKey } from "./fr";

export type { MessageKey } from "./fr";

/** Texte français pour une clé. */
export function t(key: MessageKey): string {
  let node: unknown = fr;
  for (const part of key.split(".")) {
    node = (node as Record<string, unknown>)[part];
  }
  return node as string;
}
