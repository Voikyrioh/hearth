import type { EngineCapacity } from "./capacity";
import { cpuInRange, memoryInRange } from "./scale";
import type { Resources } from "./start";

/**
 * Pourquoi les ressources ne peuvent pas partir (BR-RES-037) : `capacity` si les bornes de la machine ne sont pas
 * lues, `memory` ou `cpu` si une valeur est vide ou refusée. `null` : tout est prêt. Le formulaire qui porte le
 * composant s'en sert pour garder son bouton d'envoi inactif ; il n'existe aucune valeur « sans limite » à envoyer.
 */
export type ResourcesIssue = "capacity" | "memory" | "cpu";

export function resourcesIssue(
  capacity: EngineCapacity | null,
  value: Resources,
): ResourcesIssue | null {
  if (capacity === null) return "capacity";
  if (!memoryInRange(capacity, value.memoryBytes)) return "memory";
  if (!cpuInRange(capacity, value.cpuMillis)) return "cpu";
  return null;
}
