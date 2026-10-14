/**
 * Les heures d'une ligne (BR-SLOG-002), dans le fuseau du PC : « 14:03:22 » dans la colonne, la date et les
 * millisecondes à la demande (info-bulle, ligne d'information au clavier).
 */

const two = (n: number) => String(n).padStart(2, "0");

/** « 14:03:22 », ou un tiret quand l'heure n'est pas lisible. */
export function clockOf(t: number | null): string {
  if (t === null || !Number.isFinite(t)) return "--:--:--";
  const date = new Date(t);
  if (Number.isNaN(date.getTime())) return "--:--:--";
  return `${two(date.getHours())}:${two(date.getMinutes())}:${two(date.getSeconds())}`;
}

const DAY = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long", year: "numeric" });

/** « 13 octobre 2026, 14:03:22.481 », ou `null` quand l'heure n'est pas lisible. */
export function fullTimeOf(t: number | null): string | null {
  if (t === null || !Number.isFinite(t)) return null;
  const date = new Date(t);
  if (Number.isNaN(date.getTime())) return null;
  return `${DAY.format(date)}, ${clockOf(t)}.${String(date.getMilliseconds()).padStart(3, "0")}`;
}
