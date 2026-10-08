import { t } from "@/i18n";

/**
 * Formats des nombres du tableau de bord (BR-DASH-014) : pourcentages entiers (`87 %`), quantités
 * en Go à une décimale à virgule sans « ,0 » (`12,5 Go`, `64 Go`, `4 To` dès 1 024 Go), débits à l'unité adaptée (`1,2 Mo/s`, `450 Ko/s`), durée de
 * fonctionnement longue (`3 j 4 h 12 min`, `2 h 30 min` sous un jour). Les puissances sont celles
 * de 1024 : une mémoire de « 16 Go » est une barrette de 16 Gio. Une mesure absente se dit
 * « Non disponible » (BR-DASH-008), jamais zéro. Les unités et les gabarits sont dans `i18n/fr.ts`.
 *
 * Pourcentages et températures sont TRONQUÉS, pas arrondis : les seuils (85, 95, 80, 90) sont des
 * entiers et le niveau est décidé sur la valeur exacte, donc « 85 % » s'affiche si et seulement si
 * le seuil d'attention est atteint (84,6 % reste « 84 % », sans marque).
 */

const KIB = 1024;
const MIB = KIB * 1024;
const GIB = MIB * 1024;

export function formatPercent(value: number | null): string {
  return value === null ? t("dash.unavailable") : t("dash.unitPercent", { n: Math.floor(value) });
}

const FRENCH_NUMBER = new Intl.NumberFormat("fr-FR", {
  minimumFractionDigits: 0,
  maximumFractionDigits: 1,
});

/**
 * Une décimale, VIRGULE française, sans « ,0 » inutile, milliers séparés par une espace insécable (U+00A0, présente dans la police à chasse fixe) :
 * `64`, `12,5`, `1 023` (HRT-46, C12). FIX:01M4DPR4FFVCMZN0KPEA4JVCC8
 */
export function decimal(value: number): string {
  // DM Mono n'a pas l'espace fine insécable (U+202F, celle d'Intl en français) : les chiffres suivants tombaient dans une police de repli.
  // L'espace insécable ordinaire (U+00A0) existe dans la police ; un seul endroit : ce formateur (BR-DASH-014).
  return FRENCH_NUMBER.format(Number(value.toFixed(1))).replaceAll(" ", " ");
}

/**
 * « 12,5 Go » ; à partir de 1 024 Go, en To (`4 To`, `1,5 To`). Tout est binaire, comme l'Explorateur
 * Windows : 1 To = 1 024 Go, libellés « Go » et « To ». Un seul formateur pour la mémoire et les disques.
 * La bascule se décide sur la valeur ARRONDIE : jamais « 1024 Go ».
 */
export function formatGb(bytes: number | null): string {
  if (bytes === null) return t("dash.unavailable");
  const gb = bytes / GIB;
  return Number(gb.toFixed(1)) >= 1024
    ? t("dash.unitTb", { n: decimal(gb / 1024) })
    : t("dash.unitGb", { n: decimal(gb) });
}

/** « 12,5 Go / 64 Go » : utilisé sur total. */
export function formatUsage(used: number | null, total: number | null): string {
  if (used === null || total === null) return t("dash.unavailable");
  return t("dash.usage", { used: formatGb(used), total: formatGb(total) });
}

/** Débit adapté à son ordre de grandeur, choisi APRÈS l'arrondi ; zéro s'affiche (BR-DASH-014). */
export function formatRate(bytesPerSecond: number | null): string {
  if (bytesPerSecond === null) return t("dash.unavailable");
  const kilo = Math.round(bytesPerSecond / KIB);
  const mega = (bytesPerSecond / MIB).toFixed(1);
  const giga = (bytesPerSecond / GIB).toFixed(1);
  if (Number(mega) >= 1024) return t("dash.rateGb", { n: decimal(Number(giga)) });
  if (kilo >= 1024) return t("dash.rateMb", { n: decimal(Number(mega)) });
  if (bytesPerSecond >= KIB) return t("dash.rateKb", { n: kilo });
  return t("dash.rateB", { n: Math.round(bytesPerSecond) });
}

export function formatTemperature(celsius: number | null): string {
  return celsius === null
    ? t("dash.unavailable")
    : t("dash.unitCelsius", { n: Math.floor(celsius) });
}

export function formatFrequency(megahertz: number | null): string {
  if (megahertz === null) return t("dash.unavailable");
  return megahertz >= 1000
    ? t("dash.unitGhz", { n: decimal(megahertz / 1000) })
    : t("dash.unitMhz", { n: Math.round(megahertz) });
}

/** « 3 j 4 h 12 min », « 2 h 30 min » (moins d'un jour), « 12 min » (moins d'une heure). */
export function formatUptime(seconds: number | null): string {
  if (seconds === null) return t("dash.unavailable");
  const total = Math.max(0, Math.floor(seconds / 60));
  const days = Math.floor(total / 1440);
  const hours = Math.floor((total % 1440) / 60);
  const minutes = total % 60;
  if (days > 0) return t("dash.uptimeDays", { d: days, h: hours, m: minutes });
  if (hours > 0) return t("dash.uptimeHours", { h: hours, m: minutes });
  return t("dash.uptimeMinutes", { m: minutes });
}

/** Durée couverte, pour « Depuis 12 min » : secondes sous une minute, puis minutes entières. */
export function formatCovered(milliseconds: number): string {
  const seconds = Math.max(1, Math.floor(milliseconds / 1000));
  if (seconds < 60) return t("dash.coveredSeconds", { n: seconds });
  const minutes = Math.floor(seconds / 60);
  return minutes >= 60
    ? t("dash.uptimeHours", { h: Math.floor(minutes / 60), m: minutes % 60 })
    : t("dash.uptimeMinutes", { m: minutes });
}
