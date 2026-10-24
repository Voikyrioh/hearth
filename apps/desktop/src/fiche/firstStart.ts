import { t } from "@/i18n";
import type { FicheFirstStart } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import { proofOf } from "./proof";

/**
 * Le premier démarrage long d'un serveur de jeu (HRT-102, BR-FICHE-056), des fonctions pures sans E/S. Hearth ne connaît
 * pas la progression réelle du jeu : il ne dit QUE des faits (l'étape que la fiche sait reconnaître dans les journaux,
 * les octets de fichiers de jeu déjà écrits). Jamais de barre, de pourcentage, de durée restante ni de compteur qui
 * défile. Quand la durée annoncée par la fiche est dépassée, Hearth dit qu'il ne peut pas savoir si le jeu est prêt, et
 * ne touche à rien.
 */

export interface FirstStartFacts {
  /** Le libellé de l'étape vue dans les journaux (l'une de celles de la fiche), `null` : aucune vue. */
  stage: string | null;
  /** Les octets de fichiers de jeu déjà écrits, relevés toutes les 30 secondes ; `null` : aucun relevé. */
  writtenBytes: number | null;
  /** Depuis combien de secondes le service a démarré. */
  elapsedSeconds: number;
}

export type FirstStartView =
  /** Le panneau : une phrase par fait connu, dans l'ordre de la spec. */
  | { kind: "panel"; lines: string[] }
  /** La durée annoncée est dépassée : le panneau est remplacé, rien n'est arrêté ni relancé. */
  | { kind: "unknown"; text: string };

/** La taille totale attendue, seulement quand la fiche la connaît AVEC une preuve. */
export function expectedBytes(firstStart: FicheFirstStart): number | null {
  const sizes = firstStart.downloads.map((download) =>
    download.bytes !== null && proofOf(download.evidence) !== "unmeasured" ? download.bytes : null,
  );
  if (sizes.length === 0 || sizes.some((size) => size === null)) return null;
  return sizes.reduce<number>((sum, size) => sum + (size ?? 0), 0);
}

export function firstStartView(
  firstStart: FicheFirstStart,
  facts: FirstStartFacts,
): FirstStartView {
  if (facts.elapsedSeconds > firstStart.readyTimeoutSeconds) {
    return { kind: "unknown", text: t("fiche.firstStart.unknown") };
  }
  const lines = [t("fiche.firstStart.headline")];
  // Seulement quand la fiche déclare des repères ET que Hearth en a vu un.
  if (
    firstStart.stages.length > 0 &&
    facts.stage !== null &&
    firstStart.stages.includes(facts.stage)
  ) {
    lines.push(t("fiche.firstStart.stage", { stage: facts.stage }));
  }
  if (facts.writtenBytes !== null) {
    const total = expectedBytes(firstStart);
    lines.push(
      total === null
        ? t("fiche.firstStart.written", { size: formatSize(facts.writtenBytes) })
        : t("fiche.firstStart.writtenOf", {
            size: formatSize(facts.writtenBytes),
            total: formatSize(total),
          }),
    );
  }
  return { kind: "panel", lines };
}
