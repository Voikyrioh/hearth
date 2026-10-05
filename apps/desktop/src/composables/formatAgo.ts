import { t } from "@/i18n";

/** « il y a 2 heures », « il y a 3 jours »… d'après l'âge d'un instant (ms). */
export function formatAgo(timestamp: number, now: number): string {
  const minutes = Math.max(0, Math.floor((now - timestamp) / 60_000));
  if (minutes < 1) return t("updates.agoNow");
  if (minutes < 60) return t("updates.agoMinutes", { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return hours === 1 ? t("updates.agoHour") : t("updates.agoHours", { n: hours });
  const days = Math.floor(hours / 24);
  return days === 1 ? t("updates.agoDay") : t("updates.agoDays", { n: days });
}
