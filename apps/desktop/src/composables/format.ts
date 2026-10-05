import { t } from "@/i18n";

/** « Vu il y a 12 s », « Vu il y a 2 min »… d'après l'âge d'un dernier contact. */
export function formatSeen(lastContactAt: number | null, now: number): string {
  if (lastContactAt === null) return t("link.neverSeen");
  const seconds = Math.max(0, Math.floor((now - lastContactAt) / 1000));
  if (seconds < 60) return t("link.seenSeconds", { n: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t("link.seenMinutes", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t("link.seenHours", { n: hours });
  return t("link.seenDays", { n: Math.floor(hours / 24) });
}

/** Heure locale au format français court : « 14h23 ». */
export function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(date.getHours())}h${two(date.getMinutes())}`;
}

/** Initiales d'un nom de serveur : deux mots = leurs initiales, sinon les deux premières lettres. */
export function initials(name: string): string {
  const words = name
    .trim()
    .split(/[\s\-_.]+/)
    .filter(Boolean);
  const [first, second] = words;
  if (first && second) return (first.charAt(0) + second.charAt(0)).toUpperCase();
  return (first ?? "?").slice(0, 2).toUpperCase();
}
