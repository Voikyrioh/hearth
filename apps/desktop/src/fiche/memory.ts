import { t } from "@/i18n";
import type { Evidence } from "@/link";
import { formatMemory } from "@/resources/format";
import { proofOf } from "./proof";

/**
 * La mémoire conseillée d'une fiche en une phrase qui nomme d'où vient le chiffre (HRT-237, HRT-241, BR-FICHE-064), une
 * fonction pure. Elle ne dit jamais qu'un jeu « tient » ou « ne tient pas » : ce jugement reste à l'étape « Mémoire et
 * processeur ». Un chiffre de l'éditeur ne cite jamais la machine (il n'est pas mesuré par Hearth).
 */

export interface MemoryHint {
  recommendedBytes: number | null;
  evidence: Evidence;
}

/** `card` : la phrase de la carte (« conseillés ») ; `fact` : le fait de la fiche (« il faut »). */
export type MemoryMode = "card" | "fact";

export function memorySentence(
  memory: MemoryHint | null | undefined,
  capacityBytes: number | null,
  serverName: string,
  mode: MemoryMode,
): string {
  if (!memory || memory.recommendedBytes === null) return t("fiche.memory.unverified");
  const proof = proofOf(memory.evidence);
  if (proof === "unmeasured") return t("fiche.memory.unverified");
  const n = formatMemory(memory.recommendedBytes);
  if (proof === "publisher") return t("fiche.memory.publisher", { n });
  const tail =
    capacityBytes === null
      ? ""
      : t("fiche.memory.serverHas", { server: serverName, m: formatMemory(capacityBytes) });
  if (proof === "measured_idle") return t("fiche.memory.idle", { n }) + tail;
  return t(mode === "card" ? "fiche.memory.advised" : "fiche.memory.needs", { n }) + tail;
}
