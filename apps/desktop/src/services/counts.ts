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

/** « En erreur » regroupe « En erreur » et « Redémarre en boucle » ; « en transition » : Démarrage, Arrêt en cours, Déploiement en cours, Modification en cours (BR-SVC-011, BR-ENGINE-004) ; « autres » : En pause, État non reconnu. */
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
    case "updating":
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

/** Le filtre par état de la liste (BR-SVC-011) : « Tous », « En marche », « Arrêté », « En erreur ». */
export type StateFilter = "all" | "running" | "stopped" | "error";

/**
 * Un service est-il visible sous ce filtre ? « En erreur » regroupe « En erreur » et « Redémarre en boucle ».
 * Un service en transition (« Démarrage », « Arrêt en cours », « Déploiement en cours », « Modification en
 * cours »), « En pause » et « État non reconnu » ne sont visibles que sous « Tous » (BR-SVC-011). `acting` : une
 * action de l'utilisateur tient le service, qui reste visible sous le filtre actif jusqu'à la fin de l'action.
 */
export function visibleUnder(filter: StateFilter, state: ServiceState, acting = false): boolean {
  if (filter === "all") return true;
  if (acting) return true;
  return categoryOf(state) === filter;
}
