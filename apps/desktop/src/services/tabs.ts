/**
 * Les onglets du détail d'un service, dans l'ordre de l'écran : Vue d'ensemble, Réglages, Mods, Console, Sauvegardes,
 * Journaux (BR-CONSOLE-002). Le nom dans l'adresse (`?onglet=…`) est celui de l'écran, en français ; toute valeur
 * inconnue ouvre la vue d'ensemble.
 */
export type ServiceTab = "detail" | "settings" | "mods" | "console" | "backups" | "logs";

export const TAB_QUERY = {
  logs: "journaux",
  settings: "reglages",
  backups: "sauvegardes",
  mods: "mods",
  console: "console",
} as const;

/** L'onglet que dit la valeur de `?onglet=` ; `detail` pour tout le reste. */
export function tabOfQuery(raw: unknown): ServiceTab {
  const value = Array.isArray(raw) ? raw[0] : raw;
  for (const [tab, word] of Object.entries(TAB_QUERY)) {
    if (value === word) return tab as ServiceTab;
  }
  return "detail";
}
