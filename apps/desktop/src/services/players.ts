import { safeText } from "@/audit/text";
import { type MessageKey, t } from "@/i18n";
import type { PlayersView } from "@/link";

/**
 * Les joueurs connectés d'un serveur de jeu (HRT-291, `BR-CONSOLE-011`) : des fonctions pures. L'agent ne rend jamais un
 * chiffre deviné : `online`, `max` et les pseudos n'existent que pour l'état `ok` ; tout autre état se dit par une phrase
 * fixe, sans chiffre. Un pseudo est du texte du jeu : rendu inoffensif (`safeText`), jamais du HTML.
 */

/** Relire toutes les 30 secondes tant que l'écran est ouvert (BR-CONSOLE-011). */
export const PLAYERS_REFRESH_MS = 30_000;

const SENTENCES: Record<Exclude<PlayersView["state"], "ok">, MessageKey> = {
  stopped: "players.stopped",
  starting: "players.starting",
  unavailable: "players.unavailable",
  no_console: "players.unavailable",
  unknown: "players.unavailable",
};

/** Ce que la carte montre. */
export type PlayersShown =
  | { kind: "count"; online: number; max: number; names: string[]; more: number; hidden: number }
  | { kind: "sentence"; text: string };

/** La carte n'existe pas quand le service n'a pas de console parlée (adopté, fiche sans console). */
export function hasPlayersCard(view: PlayersView | null): boolean {
  return view !== null && view.state !== "no_console";
}

export function playersShown(view: PlayersView): PlayersShown {
  if (view.state !== "ok" || view.online === null || view.max === null) {
    const key = view.state === "ok" ? "players.unavailable" : SENTENCES[view.state];
    return { kind: "sentence", text: t(key) };
  }
  return {
    kind: "count",
    online: view.online,
    max: view.max,
    names: view.names.map((name) => safeText(name)),
    more: view.moreNames,
    hidden: view.hiddenNames,
  };
}

/** L'âge d'une lecture : « 12 s » puis « 2 min ». `null` quand la date n'est pas lisible. */
export function ageText(readAt: string | null, now: number): string | null {
  if (readAt === null) return null;
  const at = Date.parse(readAt);
  if (Number.isNaN(at)) return null;
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 60) return t("players.ageSeconds", { n: seconds });
  return t("players.ageMinutes", { n: Math.floor(seconds / 60) });
}
