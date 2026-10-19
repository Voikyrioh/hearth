import { t } from "@/i18n";
import { GIB, MIB } from "./scale";

/**
 * Unités lisibles des deux curseurs (HRT-146). Le fil reste en octets et en millicœurs ; ici naissent « 640 Mo »,
 * « 1,5 Go », « 2,5 cœurs ». Moins de 1 024 Mo : en Mo ; sinon en Go, jusqu'à deux décimales sans zéro inutile.
 */

const SHOWN = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 2 });
const TYPED = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 3, useGrouping: false });

/** Espace insécable ordinaire plutôt que fine (la police à chasse fixe n'a pas la seconde). */
function shown(value: number): string {
  return SHOWN.format(value).replaceAll(" ", " ");
}

export function formatMemory(bytes: number): string {
  return bytes < GIB
    ? t("resources.unitMb", { n: shown(bytes / MIB) })
    : t("resources.unitGb", { n: shown(bytes / GIB) });
}

/** « 1 cœur », « 2,5 cœurs » : le pluriel commence à 2. */
export function formatCpu(millis: number): string {
  const cores = millis / 1000;
  return t(cores < 2 ? "resources.unitCore" : "resources.unitCores", { n: shown(cores) });
}

export type MemoryUnit = "mo" | "go";

/** Ce que la saisie écrit pour une valeur : le nombre et l'unité qui la rendent lisible (« 640 » Mo, « 1,5 » Go). */
export function memoryFieldOf(bytes: number): { text: string; unit: MemoryUnit } {
  if (bytes >= GIB) {
    const text = TYPED.format(bytes / GIB);
    // En Go seulement si le nombre écrit se relit à l'octet près : 1025 Mo ne devient pas « 1,001 Go ».
    if (memoryFromField(text, "go") === bytes) return { text, unit: "go" };
  }
  return { text: TYPED.format(bytes / MIB), unit: "mo" };
}

export function cpuFieldOf(millis: number): string {
  return TYPED.format(millis / 1000);
}

/** Un nombre tapé : chiffres, virgule ou point (BR-RES-004). Rien d'autre : pas de signe, pas de notation savante. */
function typedNumber(text: string): number | null {
  const clean = text.trim().replace(",", ".");
  return /^\d+(\.\d+)?$/.test(clean) ? Number(clean) : null;
}

/** La mémoire tapée, en octets ; `null` si ce n'est pas un nombre ou pas un nombre entier de Mo. */
export function memoryFromField(text: string, unit: MemoryUnit): number | null {
  const n = typedNumber(text);
  if (n === null) return null;
  const mib = unit === "go" ? n * 1024 : n;
  const rounded = Math.round(mib);
  return Math.abs(mib - rounded) < 1e-6 ? rounded * MIB : null;
}

/** Le processeur tapé, en millicœurs ; `null` si ce n'est pas un nombre ou pas un nombre entier de millicœurs. */
export function cpuFromField(text: string): number | null {
  const n = typedNumber(text);
  if (n === null) return null;
  const millis = Math.round(n * 1000);
  return Math.abs(n * 1000 - millis) < 1e-6 ? millis : null;
}
