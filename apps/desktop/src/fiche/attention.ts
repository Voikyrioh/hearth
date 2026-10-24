import type { DeployCheck, FicheDetail } from "@/link";
import type { EngineCapacity, Resources } from "@/resources";
import { availableMemory, diskVerdict, type Lack, minimumVerdicts, publisherAbove } from "./fit";
import { proven } from "./proof";

/**
 * Les points d'attention de la fenêtre « Vérifiez avant de lancer » (HRT-102, BR-FICHE-026, BR-FICHE-022, BR-FICHE-021,
 * BR-FICHE-023, BR-DEPLOY-008, BR-DEPLOY-018), des fonctions pures sans E/S. Chaque point a UNE case ; « Installer le
 * serveur de jeu » reste inactif tant qu'un point n'est pas validé. Deux points (le port réservé, le manque de place)
 * partent avec la demande, car l'agent les exige (`accept`) ; les autres sont des confirmations du client, que l'agent
 * ne demande pas. Le contrat de licence n'est PAS un point : il a sa propre fenêtre (`AgreementDialog`).
 */

export type AttentionPoint =
  /** Un ou plusieurs ports sont réservés par un service arrêté (BR-DEPLOY-008). */
  | { key: "reserved_ports"; ports: { hostPort: number; protocol: string }[] }
  /** Peu de place sur le disque des images (BR-DEPLOY-018) ou besoin annoncé par l'éditeur (BR-FICHE-023). */
  | { key: "low_disk"; freeBytes: number | null; neededBytes: number | null }
  /** La limite choisie dépasse la mémoire disponible (BR-RES-008, BR-FICHE-022). */
  | { key: "over_committed"; chosen: number; available: number }
  /** La limite choisie est sous le minimum de la fiche (BR-FICHE-022). */
  | { key: "below_minimum"; chosen: number; minimum: number }
  /** Le minimum annoncé par l'éditeur dépasse la machine (BR-FICHE-021). */
  | { key: "publisher_minimum"; lack: Lack; needed: number; has: number };

export type AttentionKey = AttentionPoint["key"];

export function attentionPoints(input: {
  detail: FicheDetail;
  capacity: EngineCapacity | null;
  check: DeployCheck | null;
  resources: Resources | null;
  /** Le serveur vient de refuser faute de place (`freeBytes` : `null` s'il ne dit pas combien). */
  forceLowDisk: { freeBytes: number | null } | null;
}): AttentionPoint[] {
  const { detail, capacity, check, resources, forceLowDisk } = input;
  const points: AttentionPoint[] = [];

  const minimums = publisherAbove(minimumVerdicts(detail, capacity));
  for (const verdict of minimums) {
    if (verdict.kind === "publisher_above") {
      points.push({
        key: "publisher_minimum",
        lack: verdict.lack,
        needed: verdict.needed,
        has: verdict.has,
      });
    }
  }

  const reserved = (check?.ports ?? []).filter((port) => port.status === "reserved_by_stopped");
  if (reserved.length > 0) {
    points.push({
      key: "reserved_ports",
      ports: reserved.map((port) => ({ hostPort: port.hostPort, protocol: port.protocol })),
    });
  }

  const free = check?.disk?.freeBytes ?? null;
  const verdict = diskVerdict(detail, free);
  const needed = proven(detail.resources.disk.installBytes, detail.resources.disk.evidence);
  if (forceLowDisk !== null) {
    points.push({ key: "low_disk", freeBytes: forceLowDisk.freeBytes, neededBytes: needed });
  } else if (verdict.kind === "short_publisher") {
    points.push({ key: "low_disk", freeBytes: verdict.free, neededBytes: verdict.needed });
  } else if (check?.disk?.low && !check.imagePresent) {
    points.push({ key: "low_disk", freeBytes: check.disk.freeBytes, neededBytes: needed });
  }

  if (capacity && resources?.memoryBytes != null) {
    const available = availableMemory(capacity);
    if (resources.memoryBytes > available) {
      points.push({
        key: "over_committed",
        chosen: resources.memoryBytes,
        available,
      });
    }
    const minimum = proven(detail.resources.memory.minBytes, detail.resources.memory.evidence);
    if (minimum !== null && resources.memoryBytes < minimum) {
      points.push({ key: "below_minimum", chosen: resources.memoryBytes, minimum });
    }
  }
  return points;
}

/** Tous les points sont-ils validés ? */
export function allValidated(
  points: readonly AttentionPoint[],
  ticked: Readonly<Record<string, boolean>>,
): boolean {
  return points.every((point) => ticked[point.key] === true);
}

/** Les cases qui partent avec la demande : seules `reserved_ports` et `low_disk` sont comprises par l'agent. */
export function acceptOf(
  points: readonly AttentionPoint[],
  ticked: Readonly<Record<string, boolean>>,
): { reservedPorts: boolean; lowDisk: boolean } {
  const done = (key: AttentionKey) =>
    points.some((point) => point.key === key) && ticked[key] === true;
  return { reservedPorts: done("reserved_ports"), lowDisk: done("low_disk") };
}
