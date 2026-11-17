import { safeText } from "@/audit/text";
import { t } from "@/i18n";
import type { FicheDetail } from "@/link";
import { formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";
import { currentAnswer } from "./answers";
import type { FicheDraft } from "./draft";
import { stopTexts } from "./messages";
import { publishedPorts, readableHostPorts } from "./ports";
import { proofOf } from "./proof";

/**
 * Ce que la page « Vérifiez avant de lancer » dit d'avance (HRT-246, BR-FICHE-025), des fonctions pures sans E/S : ce que
 * Hearth va faire dans l'ordre, l'adresse prévue pour les joueurs et la raison qui retient le bouton de lancement. Tout
 * est lu dans la fiche et la saisie ; le serveur refait chaque contrôle au lancement et a le dernier mot.
 */

/** Le nom du monde que le jeu va créer : la réponse du réglage figé à la création, quand la fiche en nomme un. */
export function createdWorld(detail: FicheDetail, draft: FicheDraft): string | null {
  const setting = detail.settings.find(
    (item) => item.question.frozenAfterCreation && !item.question.secret,
  );
  if (!setting || draft.world.resume) return null;
  const name = currentAnswer(setting.question, draft.answers).trim();
  return name === "" ? null : safeText(name);
}

/** Les quatre étapes de « Ce qui va se passer », dans l'ordre où Hearth les fait. */
export function whatWillHappen(
  detail: FicheDetail,
  draft: FicheDraft,
  serverName: string,
): string[] {
  const disk = detail.resources.disk;
  const size =
    disk.installBytes !== null && proofOf(disk.evidence) !== "unmeasured"
      ? t("fiche.review.size", { size: formatSize(disk.installBytes) })
      : "";
  const memory =
    draft.resources?.memoryBytes != null ? formatMemory(draft.resources.memoryBytes) : "";
  const cpu = draft.resources?.cpuMillis != null ? formatCpu(draft.resources.cpuMillis) : "";
  const world = createdWorld(detail, draft);
  const timeout = detail.firstStart?.readyTimeoutSeconds ?? null;
  const worldPart = world === null ? "" : t("fiche.review.step3World", { world });
  return [
    t("fiche.review.step1", { game: safeText(detail.card.name), size }),
    t("fiche.review.step2", {
      name: safeText(draft.name),
      server: serverName,
      memory,
      cpu,
    }),
    timeout === null
      ? t("fiche.review.step3Unknown", { world: worldPart })
      : t("fiche.review.step3", { world: worldPart, d: Math.max(1, Math.ceil(timeout / 60)) }),
    t("fiche.review.step4"),
  ];
}

/**
 * La phrase qui suit la liste : ce que la fiche dit de l'enregistrement du monde à l'arrêt (la MÊME phrase que la ligne
 * « Arrêt du jeu » : une seule vérité, BR-JEU-002), puis l'annulation. Une fiche sans arrêt déclaré ne promet rien.
 */
export function whatWillHappenNote(detail: FicheDetail): string {
  const saving = detail.stop ? (stopTexts(detail.stop)[0] ?? "") : "";
  return (saving === "" ? "" : `${saving} `) + t("fiche.review.noteCancel");
}

export interface PlannedAddress {
  /** « 192.168.1.20:7777 » : pour les joueurs du même réseau que le serveur. */
  lan: string | null;
  /** Les ports à ouvrir sur la box, un par port publié : « 7777 (UDP) ». */
  internet: { port: number; protocol: string }[];
}

/** L'adresse prévue pour les joueurs : celle du réseau maison, et les ports à ouvrir pour internet. */
export function plannedAddress(
  detail: FicheDetail,
  draft: FicheDraft,
  host: string,
): PlannedAddress {
  const published = publishedPorts(detail.ports, draft.optionalPorts, draft.variant);
  const lines = readableHostPorts(published, draft.hostPorts);
  const game = lines.find((line) => line.port.role === "game") ?? lines[0];
  const shown = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return {
    lan: game ? `${safeText(shown)}:${game.deploy.hostPort}` : null,
    internet: lines.map((line) => ({
      port: line.deploy.hostPort,
      protocol: line.deploy.protocol.toUpperCase(),
    })),
  };
}

/**
 * Pourquoi le bouton de lancement est inactif ; `null` quand il est actif (BR-FICHE-025). Les points d'attention
 * attendent une case chacun, le mot de passe attend sa saisie : la phrase dit les deux quand les deux manquent.
 */
export function launchReason(attentionPending: boolean, passwordMissing: boolean): string | null {
  if (attentionPending && passwordMissing) return t("fiche.review.reasonBoth");
  if (attentionPending) return t("fiche.review.reasonAttention");
  if (passwordMissing) return t("fiche.review.reasonPassword");
  return null;
}
