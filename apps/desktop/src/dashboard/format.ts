import { t } from "@/i18n";

/**
 * Formats des nombres du tableau de bord (BR-DASH-014) : pourcentages entiers (`87 %`), quantités
 * en Go à une décimale (`12.5 Go`), débits à l'unité adaptée (`1.2 Mo/s`, `450 Ko/s`), durée de
 * fonctionnement longue (`3 j 4 h 12 min`, `2 h 30 min` sous un jour). Les puissances sont celles
 * de 1024 : une mémoire de « 16 Go » est une barrette de 16 Gio. Une mesure absente se dit
 * « Non disponible » (BR-DASH-008), jamais zéro.
 */

const KIB = 1024;
const MIB = KIB * 1024;
const GIB = MIB * 1024;

export function formatPercent(value: number | null): string {
  return value === null ? t("dash.unavailable") : `${Math.round(value)} %`;
}

export function formatGb(bytes: number | null): string {
  return bytes === null ? t("dash.unavailable") : `${(bytes / GIB).toFixed(1)} Go`;
}

/** « 12.5 Go / 64.0 Go » : utilisé sur total. */
export function formatUsage(used: number | null, total: number | null): string {
  if (used === null || total === null) return t("dash.unavailable");
  return `${formatGb(used)} / ${formatGb(total)}`;
}

/** Débit adapté à son ordre de grandeur ; zéro s'affiche (BR-DASH-014). */
export function formatRate(bytesPerSecond: number | null): string {
  if (bytesPerSecond === null) return t("dash.unavailable");
  if (bytesPerSecond >= GIB) return `${(bytesPerSecond / GIB).toFixed(1)} Go/s`;
  if (bytesPerSecond >= MIB) return `${(bytesPerSecond / MIB).toFixed(1)} Mo/s`;
  if (bytesPerSecond >= KIB) return `${Math.round(bytesPerSecond / KIB)} Ko/s`;
  return `${Math.round(bytesPerSecond)} o/s`;
}

export function formatTemperature(celsius: number | null): string {
  return celsius === null ? t("dash.unavailable") : `${Math.round(celsius)} °C`;
}

export function formatFrequency(megahertz: number | null): string {
  if (megahertz === null) return t("dash.unavailable");
  return megahertz >= 1000
    ? `${(megahertz / 1000).toFixed(1)} GHz`
    : `${Math.round(megahertz)} MHz`;
}

/** « 3 j 4 h 12 min », « 2 h 30 min » (moins d'un jour), « 12 min » (moins d'une heure). */
export function formatUptime(seconds: number | null): string {
  if (seconds === null) return t("dash.unavailable");
  const total = Math.max(0, Math.floor(seconds / 60));
  const days = Math.floor(total / 1440);
  const hours = Math.floor((total % 1440) / 60);
  const minutes = total % 60;
  if (days > 0) return `${days} j ${hours} h ${minutes} min`;
  if (hours > 0) return `${hours} h ${minutes} min`;
  return `${minutes} min`;
}

/** Durée couverte, pour « Depuis 12 min » : minutes entières (au moins 1). */
export function formatCovered(milliseconds: number): string {
  const minutes = Math.max(1, Math.round(milliseconds / 60_000));
  return minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`;
}
