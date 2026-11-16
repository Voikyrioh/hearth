import { t } from "@/i18n";
import type { Evidence, FicheDetail } from "@/link";
import { formatMemory } from "@/resources/format";
import { formatSize } from "@/serviceOps/messages";
import { memorySentence } from "./memory";
import { proofOf } from "./proof";

/**
 * Ce que la fiche d'un jeu dit d'avance (HRT-242, BR-FICHE-004), des fonctions pures sans E/S : les quatre faits clés du
 * bandeau (mémoire, espace disque, ports, premier démarrage) et les phrases de « Ce qu'il faut pour ce jeu ». Tout est lu
 * dans la fiche que l'agent propose : jamais un chiffre sans sa preuve, jamais un repère inventé.
 */

export interface KeyFact {
  id: "memory" | "disk" | "ports" | "start";
  text: string;
}

/** Les ports que voit un joueur : jamais ceux qui ne sont jamais publiés (console à distance). */
export function publishedPorts(detail: FicheDetail): FicheDetail["ports"] {
  return detail.ports.filter((port) => port.publish !== "never");
}

function portNumber(port: FicheDetail["ports"][number]): number {
  return port.defaultHost ?? port.container;
}

/** « Un seul port à ouvrir : 7777 », « 2 ports à ouvrir : 7777, 27015 » ou « Aucun port à ouvrir ». */
export function portsFact(detail: FicheDetail): string {
  const ports = publishedPorts(detail).map(portNumber);
  if (ports.length === 0) return t("fiche.facts.portsNone");
  if (ports.length === 1) return t("fiche.facts.portsOne", { port: ports[0] ?? 0 });
  return t("fiche.facts.portsMany", { k: ports.length, list: ports.join(", ") });
}

/** La mémoire que la fiche recommande, à défaut son minimum, avec sa preuve. */
function memoryHint(detail: FicheDetail) {
  const memory = detail.resources.memory;
  const bytes = memory.recommendedBytes ?? memory.minBytes;
  return bytes === null ? null : { recommendedBytes: bytes, evidence: memory.evidence };
}

export function keyFacts(
  detail: FicheDetail,
  capacityBytes: number | null,
  serverName: string,
): KeyFact[] {
  const disk = detail.resources.disk;
  const diskText =
    disk.installBytes !== null && proofOf(disk.evidence) !== "unmeasured"
      ? t("fiche.facts.disk", { n: formatSize(disk.installBytes) })
      : t("fiche.facts.diskUnverified");
  const timeout = detail.firstStart?.readyTimeoutSeconds ?? null;
  const start =
    timeout === null
      ? t("fiche.facts.startUnknown")
      : t("fiche.facts.start", { d: Math.max(1, Math.ceil(timeout / 60)) });
  return [
    { id: "memory", text: memorySentence(memoryHint(detail), capacityBytes, serverName, "fact") },
    { id: "disk", text: diskText },
    { id: "ports", text: portsFact(detail) },
    { id: "start", text: start },
  ];
}

/** La phrase de preuve d'un chiffre (BR-FICHE-057). */
export function proofSentence(need: { evidence: Evidence }): string {
  switch (proofOf(need.evidence)) {
    case "measured":
      return t("fiche.facts.proofMeasured");
    case "measured_idle":
      return t("fiche.facts.proofIdle");
    case "publisher":
      return t("fiche.facts.proofPublisher");
    default:
      return t("fiche.facts.proofNone");
  }
}

export interface NeedLines {
  memory: string;
  disk: string;
  cpu: string;
}

/** Les trois lignes de « Place sur votre serveur » (BR-FICHE-004, 057). */
export function needLines(
  detail: FicheDetail,
  capacity: { memoryBytes: number; cpus: number } | null,
  serverName: string,
): NeedLines {
  const resources = detail.resources;
  const need = resources.memory.minBytes ?? resources.memory.recommendedBytes;
  const memory =
    need === null || proofOf(resources.memory.evidence) === "unmeasured"
      ? t("fiche.facts.lineMemoryUnverified")
      : t(capacity ? "fiche.facts.lineMemory" : "fiche.facts.lineMemoryBare", {
          n: formatMemory(need),
          server: serverName,
          m: capacity ? formatMemory(capacity.memoryBytes) : "",
          proof: proofSentence(resources.memory),
        });
  const disk =
    resources.disk.installBytes === null || proofOf(resources.disk.evidence) === "unmeasured"
      ? t("fiche.facts.lineDiskUnverified")
      : t("fiche.facts.lineDisk", {
          n: formatSize(resources.disk.installBytes),
          proof: proofSentence(resources.disk),
        });
  const cpu = capacity
    ? t("fiche.facts.lineCpu", { server: serverName, c: capacity.cpus })
    : t("fiche.facts.lineCpuBare");
  return { memory, disk, cpu };
}
