import type { ServiceInfo, ServiceState } from "@/link";

/**
 * Les compteurs de la carte du moteur (BR-ENGINE-004) : calculés par l'interface depuis les états de la
 * liste, l'agent n'envoie aucun compteur. Chaque état appartient à UNE catégorie ; le total égale le nombre de
 * services de la liste ; les services créés hors de Hearth sont comptés comme les autres.
 */
export type CountKey = "running" | "stopped" | "error" | "transition" | "other";

export const COUNT_KEYS: readonly CountKey[] = [
  "running",
  "stopped",
  "error",
  "transition",
  "other",
];

/** « En erreur » regroupe « En erreur » et « Redémarre en boucle » ; « en transition » : Démarrage, Arrêt en cours, Déploiement en cours ; « autres » : En pause, État non reconnu. */
export function categoryOf(state: ServiceState): CountKey {
  switch (state) {
    case "running":
      return "running";
    case "stopped":
      return "stopped";
    case "error":
    case "restart_loop":
      return "error";
    case "starting":
    case "stopping":
    case "deploying":
      return "transition";
    case "paused":
    case "unknown":
      return "other";
  }
}

export type Counts = Record<CountKey, number>;

export function countServices(services: readonly Pick<ServiceInfo, "state">[]): Counts {
  const counts: Counts = { running: 0, stopped: 0, error: 0, transition: 0, other: 0 };
  for (const service of services) counts[categoryOf(service.state)] += 1;
  return counts;
}

/** Les catégories à montrer : « en marche », « arrêtés » et « en erreur » toujours ; les deux autres seulement si elles ne sont pas vides. */
export function shownCounts(counts: Counts): { key: CountKey; n: number }[] {
  return COUNT_KEYS.filter(
    (key) => key === "running" || key === "stopped" || key === "error" || counts[key] > 0,
  ).map((key) => ({ key, n: counts[key] }));
}
