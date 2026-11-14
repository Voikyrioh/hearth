import { safeText } from "@/audit/text";
import type { ProgressStep } from "@/components/molecules/ProgressSteps.vue";
import { t } from "@/i18n";
import type {
  DeletePreview,
  DeletePreviewRefusal,
  DeleteRefusal,
  DeleteRequest,
  EngineState,
  FolderPreview,
  ServiceInfo,
  ServiceOperation,
  ServiceState,
} from "@/link";
import { formatCount, formatSize, launchedBy } from "@/serviceOps/messages";
import { ENGINE_STATE_KEYS } from "@/services/format";
import { graceSeconds, stateName } from "@/services/lifecycle";
import { busyText, type PilotLock } from "@/services/pilot";
import { canAct, type RightsGate } from "@/services/rights";

/**
 * Supprimer un service (HRT-86, `BR-DEL-*`) : des fonctions pures, sans E/S. L'agent reste l'arbitre à chaque demande
 * et refait tous les contrôles ; ceci dit d'avance, honnêtement, si le bouton existe, pourquoi il est inactif, ce que
 * la fenêtre montre de chaque dossier et ce que l'agent a refusé. Aucun texte ne vient de l'agent : il envoie des
 * CODES, l'interface les met en phrases. Un nom, une image ou un chemin venu du serveur est un texte inoffensif
 * (`safeText`), jamais du HTML ; un chemin n'est écrit que dans la fenêtre, jamais dans un journal ni une trace.
 */

/** Les états où le service se supprime (BR-DEL-003) : en marche, arrêté, en erreur, redémarre en boucle. */
const DELETABLE: readonly ServiceState[] = ["running", "stopped", "error", "restart_loop"];

/** Les états où une action tient déjà le service (BR-DEL-003). */
const IN_TRANSITION: readonly ServiceState[] = ["starting", "stopping", "updating"];

export function isDeletable(state: ServiceState): boolean {
  return DELETABLE.includes(state);
}

/**
 * Ce que la carte « Suppression » du détail montre (BR-DEL-002, 003, 026, BR-SVC-025) :
 * - `hidden` : pas de carte (agent d'avant la suppression, service en déploiement, service disparu ou jamais confié) ;
 * - `adopted` : la carte explique que Hearth ne supprime jamais un service adopté (BR-DEL-002) ;
 * - `ready` : le bouton est actif ;
 * - `blocked` : le bouton est visible et désactivé ; `rights` : c'est le droit (sa raison s'écrit par
 *   `ServiceRightReason`, une seule fois), sinon `reason` est la phrase de raison, avec `by` quand une action d'un
 *   autre compte tient le service.
 */
export type DeleteDoor =
  | { kind: "hidden" }
  | { kind: "adopted" }
  | { kind: "ready" }
  | { kind: "blocked"; rights: boolean; reason: string | null; by: string | null };

export function deleteDoor(input: {
  service: ServiceInfo;
  gone: boolean;
  /** L'agent annonce `service-delete` (`ServiceRights.serviceDelete`). */
  supported: boolean;
  gate: RightsGate;
  lock: PilotLock;
  engineReady: boolean;
}): DeleteDoor {
  const { service, gone, supported, gate, lock, engineReady } = input;
  if (gone || !supported) return { kind: "hidden" };
  // Le détail réduit d'un service en déploiement ne porte pas cette carte (BR-SVC-025, écart 14.17 du design).
  if (service.state === "deploying") return { kind: "hidden" };
  if (service.adopted !== null) return { kind: "adopted" };
  // Un conteneur ni créé par Hearth ni adopté n'est jamais publié par l'agent : garde défensive.
  if (service.origin !== "hearth") return { kind: "hidden" };
  if (!canAct(gate)) return { kind: "blocked", rights: true, reason: null, by: null };
  if (lock.locked) {
    return { kind: "blocked", rights: false, reason: busyText(lock), by: lock.by };
  }
  if (IN_TRANSITION.includes(service.state)) {
    return { kind: "blocked", rights: false, reason: t("serviceOps.busy"), by: null };
  }
  if (!isDeletable(service.state)) {
    return { kind: "blocked", rights: false, reason: t("lifecycle.notDrivable"), by: null };
  }
  if (!engineReady) {
    return { kind: "blocked", rights: false, reason: t("lifecycle.engineNotReady"), by: null };
  }
  return { kind: "ready" };
}

// ── La fenêtre : les dossiers et leur sort ───────────────────────────────────────────────────────

/** Les dossiers que la case « données » effacerait (BR-DEL-006 à 008). */
export function erasableFolders(preview: DeletePreview): FolderPreview[] {
  return preview.folders.filter((folder) => folder.fate === "erasable");
}

/** La case « données » est proposée : il y a au moins un dossier géré que la suppression pourrait effacer. */
export function offersDataBox(preview: DeletePreview): boolean {
  return erasableFolders(preview).length > 0;
}

/** La case « sauvegardes » est proposée : Hearth connaît des sauvegardes de ce service (BR-DEL-005). */
export function offersBackupsBox(preview: DeletePreview): boolean {
  return preview.backups > 0;
}

export function sizeCell(folder: FolderPreview): string {
  if (folder.unmeasured) return t("serviceDelete.notMeasured");
  if (folder.partial) return t("serviceDelete.moreThan", { size: formatSize(folder.bytes) });
  return formatSize(folder.bytes);
}

export function itemsCell(folder: FolderPreview): string {
  if (folder.unmeasured) return t("serviceDelete.notMeasured");
  if (folder.partial) return t("serviceDelete.moreThanItems", { n: formatCount(folder.items) });
  return formatCount(folder.items);
}

/** Le « Sort » d'un dossier géré (BR-DEL-006 à 008) : effacé seulement sur la case, jamais un dossier partagé. */
export function fateText(folder: FolderPreview, eraseData: boolean): string {
  switch (folder.fate) {
    case "erasable":
      return t(eraseData ? "serviceDelete.fate.erased" : "serviceDelete.fate.kept");
    case "kept_shared":
      return t("serviceDelete.fate.keptShared");
    default:
      return t("serviceDelete.fate.kept");
  }
}

/** Une ligne du tableau « Dossier · Taille · Éléments · Sort ». */
export interface FolderRow {
  key: string;
  path: string;
  size: string;
  items: string;
  fate: string;
  /** La suppression des données l'efface : la ligne est mise en avant. */
  erased: boolean;
}

export function folderRows(preview: DeletePreview, eraseData: boolean): FolderRow[] {
  const managed = preview.folders.map(
    (folder, index): FolderRow => ({
      key: `managed-${index}`,
      path: safeText(folder.path),
      size: sizeCell(folder),
      items: itemsCell(folder),
      fate: fateText(folder, eraseData),
      erased: eraseData && folder.fate === "erasable",
    }),
  );
  const server = preview.serverFolders.map(
    (path, index): FolderRow => ({
      key: `server-${index}`,
      path: safeText(path),
      size: t("serviceDelete.notMeasured"),
      items: t("serviceDelete.notMeasured"),
      fate: t("serviceDelete.fate.keptServer"),
      erased: false,
    }),
  );
  return [...managed, ...server];
}

/** Une ligne d'information sous le tableau, une par cas (BR-DEL-008, 009). */
export interface FolderNote {
  key: string;
  text: string;
}

function namesText(folder: FolderPreview): string {
  const names = folder.sharedWith.map((name) => safeText(name));
  if (folder.sharedWithUnmanaged) names.push(t("serviceDelete.unmanagedContainer"));
  if (names.length === 0) return t("serviceDelete.unmanagedContainer");
  return names.length === 1
    ? (names[0] ?? "")
    : t("serviceDelete.nameList", {
        all: names.slice(0, -1).join(", "),
        last: names[names.length - 1] ?? "",
      });
}

export function folderNotes(preview: DeletePreview): FolderNote[] {
  const notes: FolderNote[] = [];
  preview.folders.forEach((folder, index) => {
    const path = safeText(folder.path);
    if (folder.fate === "kept_shared") {
      notes.push({
        key: `shared-${index}`,
        text: t("serviceDelete.noteShared", { path, services: namesText(folder) }),
      });
    } else if (folder.fate === "kept_unverifiable") {
      notes.push({
        key: `unverifiable-${index}`,
        text: t("serviceDelete.noteUnverifiable", { path }),
      });
    } else if (folder.fate === "kept_outside_root") {
      notes.push({ key: `outside-${index}`, text: t("serviceDelete.noteOutsideRoot", { path }) });
    }
  });
  return notes;
}

/**
 * Une phrase par dossier dont la mesure n'a pas tenu dans les 10 secondes : « Taille non mesurée pour {chemin} : plus
 * grande que {taille}. » (BR-DEL-009). Chaque phrase se rattache à sa ligne par le chemin (revue UX de Nora, constat 4).
 */
export function unmeasuredNotes(preview: DeletePreview): FolderNote[] {
  const notes: FolderNote[] = [];
  preview.folders.forEach((folder, index) => {
    if (!folder.partial || folder.unmeasured) return;
    notes.push({
      key: `partial-${index}`,
      text: t("serviceDelete.sizeNotMeasured", {
        path: safeText(folder.path),
        size: formatSize(folder.bytes),
      }),
    });
  });
  return notes;
}

/** La taille d'un dossier dans une phrase : « 1,2 Go », « plus de 6 Go », « non mesurée » (jamais de parenthèse imbriquée). */
function sizeInSentence(folder: FolderPreview): string {
  if (folder.unmeasured) return t("serviceDelete.notMeasuredShort");
  if (folder.partial) return t("serviceDelete.moreThanShort", { size: formatSize(folder.bytes) });
  return formatSize(folder.bytes);
}

/** « {chemin} ({taille}), … » : la liste lue dans les phrases d'effet. */
export function folderList(folders: readonly FolderPreview[]): string {
  return folders.map((folder) => `${safeText(folder.path)} (${sizeInSentence(folder)})`).join(", ");
}

/** Le délai d'arrêt écrit (« 10 secondes »), ou la forme sans chiffre quand l'agent ne l'a pas dit. */
export function runningText(preview: DeletePreview): string {
  const delay = graceSeconds({ graceS: preview.stopGraceS });
  return delay === null ? t("serviceDelete.runningNoDelay") : t("serviceDelete.running", { delay });
}

/** L'effet de la suppression, dit dans tous les cas (BR-DEL-004). */
export function effectText(preview: DeletePreview): string {
  return t("serviceDelete.effect", {
    name: safeText(preview.name),
    image: safeText(preview.image),
  });
}

// ── La confirmation ──────────────────────────────────────────────────────────────────────────────

/** Le nom retapé est exigé dès qu'une case est cochée (BR-DEL-011). */
export function mustRetype(boxes: { eraseData: boolean; eraseBackups: boolean }): boolean {
  return boxes.eraseData || boxes.eraseBackups;
}

/** Le nom retapé est exactement celui du service, casse comprise (BR-DEL-011). */
export function nameMatches(typed: string, name: string): boolean {
  return typed === name;
}

/** La demande que la fenêtre envoie : jamais le nom retapé quand aucune case n'est cochée. */
export function requestOf(
  boxes: { eraseData: boolean; eraseBackups: boolean },
  typed: string,
): DeleteRequest {
  return {
    eraseData: boxes.eraseData,
    eraseBackups: boxes.eraseBackups,
    confirmName: mustRetype(boxes) ? typed : null,
  };
}

/** Le service a changé sous la fenêtre de confirmation (BR-LIFE-022, BR-DEL-022) : de quoi désactiver la confirmation. */
export function staleNotice(
  current: ServiceInfo | null,
  locked: boolean,
  engineReady: boolean,
): string | null {
  if (current === null) return t("serviceDetail.gone");
  if (!isDeletable(current.state)) {
    return t("lifecycle.stateChanged", { state: stateName(current.state) });
  }
  if (locked) return t("serviceOps.busy");
  if (!engineReady) return t("lifecycle.engineNotReady");
  return null;
}

function engineSentence(engine: EngineState | null): string {
  const key = engine && engine !== "checking" ? ENGINE_STATE_KEYS[engine] : undefined;
  return key
    ? t("serviceDelete.refusedEngine", { state: t(key) })
    : t("serviceDelete.refusedEngineNoState");
}

/** Le texte d'un refus de la consultation (BR-DEL-002, 018, 022). */
export function previewRefusalText(refusal: DeletePreviewRefusal): string {
  switch (refusal.kind) {
    case "service_not_found":
      return t("serviceDelete.refusedGone");
    case "service_adopted":
      return t("serviceDelete.refusedAdopted");
    case "engine_unavailable":
      return engineSentence(refusal.engine);
    default:
      return t("serviceDelete.previewRefused");
  }
}

/**
 * Le texte d'un refus de l'agent hors de la confirmation (BR-DEL-002, 003, 011, 021, 022). La confirmation se dit
 * sous son champ. Un code que ce client ne connaît pas devient le refus générique.
 */
export function refusalText(refusal: { kind: string }): string {
  const shown = refusal as DeleteRefusal;
  switch (shown.kind) {
    case "engine_unavailable":
      return engineSentence(shown.engine);
    case "service_not_found":
      return t("serviceDelete.refusedGone");
    case "service_adopted":
      return t("serviceDelete.refusedAdopted");
    case "service_busy": {
      const who = launchedBy(shown.by);
      return who === null ? t("serviceOps.busy") : `${t("serviceOps.busy")} ${who}`;
    }
    case "not_drivable":
      return t("lifecycle.notDrivable");
    case "name_mismatch":
      return t("serviceDelete.nameError");
    case "backups_unavailable":
      return t("serviceDelete.refusedBackups");
    default:
      return t("serviceDelete.refusedOther");
  }
}

/** Le texte d'un refus d'annulation qui n'est pas celui de la confirmation (le mot de passe se dit sous son champ). */
export function cancelRefusalText(refusal: { kind: string }): string {
  switch (refusal.kind) {
    case "not_found":
      return t("serviceDelete.cancelNotFound");
    case "too_late":
      return t("serviceDelete.tooLate");
    case "not_cancellable":
      return t("serviceDelete.cancelNotCancellable");
    case "changed":
      return t("serviceDelete.cancelChanged");
    default:
      return t("serviceDelete.cancelOther");
  }
}

// ── La progression et l'issue ────────────────────────────────────────────────────────────────────

/** Les étapes que ce client montre, dans l'ordre du plan : « remove_container » et « forget » sont une seule ligne. */
export type DeleteStage = "stop" | "remove" | "erase";

const STAGE_OF: Record<string, DeleteStage> = {
  stop: "stop",
  remove_container: "remove",
  forget: "remove",
  erase: "erase",
};

const STAGE_KEYS = {
  stop: "serviceDelete.steps.stop",
  remove: "serviceDelete.steps.remove",
  erase: "serviceDelete.steps.erase",
} as const;

/** Les étapes du plan : l'arrêt seulement si le service tournait, l'effacement seulement si la case était cochée. */
export function stagesOf(plan: { stop: boolean; erase: boolean }): DeleteStage[] {
  return [
    ...(plan.stop ? (["stop"] as const) : []),
    "remove",
    ...(plan.erase ? (["erase"] as const) : []),
  ];
}

export function stepsOf(
  operation: ServiceOperation | null,
  plan: { stop: boolean; erase: boolean },
): ProgressStep[] {
  const stages = stagesOf(plan);
  const current = operation?.step ? STAGE_OF[operation.step] : undefined;
  const index = Math.max(0, current ? stages.indexOf(current) : 0);
  return stages.map((stage, position): ProgressStep => {
    const state = position < index ? "done" : position === index ? "now" : "later";
    const step: ProgressStep = { key: stage, label: t(STAGE_KEYS[stage]), state };
    if (stage === "erase" && state === "now" && operation) {
      step.bar = {
        ratio: null,
        text:
          operation.itemsDone === null
            ? null
            : t("serviceOps.itemsErased", { n: formatCount(operation.itemsDone) }),
      };
    }
    return step;
  });
}

/** Pourquoi la suppression a échoué avant le point de non-retour (BR-DEL-015) : la cause, jamais un texte de l'agent. */
export function failureCauseText(cause: ServiceOperation["cause"]): string | null {
  switch (cause) {
    case "engine_unavailable":
      return t("serviceDelete.cause.engineUnavailable");
    case "engine_refused":
      return t("serviceDelete.cause.engineRefused");
    case "timeout":
      return t("serviceDelete.cause.timeout");
    case "service_not_found":
      return t("serviceDelete.refusedGone");
    default:
      return null;
  }
}

/** L'issue d'une suppression terminée, telle que la fenêtre la dit. */
export type DeleteEnd =
  | { kind: "success"; forced: boolean }
  | { kind: "partial"; forced: boolean }
  | { kind: "failed" }
  | { kind: "cancelled"; stopped: boolean }
  | { kind: "interrupted" };

export function endOf(operation: ServiceOperation): DeleteEnd {
  switch (operation.status) {
    case "succeeded":
      return operation.cause === "data_not_erased"
        ? { kind: "partial", forced: false }
        : { kind: "success", forced: operation.cause === "forced_stop" };
    case "cancelled": {
      const stopped = operation.steps.some((step) => step.step === "stop" && step.state === "done");
      return { kind: "cancelled", stopped };
    }
    case "interrupted":
      return { kind: "interrupted" };
    default:
      return { kind: "failed" };
  }
}
