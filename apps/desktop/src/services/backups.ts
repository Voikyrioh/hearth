import { isHearthUsername } from "@/accounts/name";
import { safeText } from "@/audit/text";
import { formatDateTime } from "@/composables/format";
import { type MessageKey, t } from "@/i18n";
import type {
  BackupFlag,
  BackupInfo,
  BackupMode,
  BackupPreview,
  BackupReadRefusal,
  BackupRefusal,
  BackupRequest,
  BackupsView,
  BigFile,
  EngineState,
  IgnoredItem,
  ServiceInfo,
  ServiceState,
} from "@/link";
import { formatCount, formatSize } from "@/serviceOps/messages";
import { ENGINE_STATE_KEYS } from "@/services/format";
import { busyText, type PilotLock } from "@/services/pilot";
import { canAct, type RightsGate } from "@/services/rights";

/**
 * Les sauvegardes d'un service (HRT-98, `BR-BACKUP-*`) : des fonctions pures, sans E/S. L'agent reste l'arbitre à chaque
 * demande et refait tous les contrôles ; ceci dit d'avance, honnêtement, si l'onglet existe, pourquoi un bouton est
 * inactif, ce que la liste montre de chaque sauvegarde et ce que l'agent a refusé. Aucun texte ne vient de l'agent : il
 * envoie des CODES, l'interface les met en phrases. Un nom, une note ou un chemin venu du serveur est un texte
 * inoffensif (`safeText`), jamais du HTML ; la note d'un compte n'est écrite dans aucune trace ni aucun journal.
 */

/** La note d'une sauvegarde : 80 caractères au plus (BR-BACKUP-012). */
export const NOTE_MAX = 80;
/** Au plus ce nombre d'éléments dans la liste des éléments ignorés d'un service (miroir de l'agent). */
export const MAX_ITEMS = 500;
/** Au-delà de ce nombre de sauvegardes, l'onglet avertit (BR-BACKUP-015). */
export const WARN_ABOVE = 10;
/** Sous cet espace libre, le disque des sauvegardes est dit presque plein (la réserve de l'agent est d'au moins 5 Gio). */
export const ALMOST_FULL_BYTES = 5 * 1024 ** 3;

/** Les états où une sauvegarde se crée : l'agent juge `check_state` (BR-BACKUP-006) ; ceci le dit d'avance. */
const SAVABLE: readonly ServiceState[] = ["running", "stopped", "error", "restart_loop"];

/**
 * L'onglet « Sauvegardes » du détail (BR-BACKUP-028, BR-SVC-025) :
 * - `hidden` : pas d'onglet (agent d'avant, service disparu, en déploiement, ou non créé par Hearth : l'agent refuse
 *   encore les sauvegardes d'un service adopté, HRT-70) ;
 * - `shown` : l'onglet existe, la liste est lisible par tout compte.
 */
export type BackupsDoor = { kind: "hidden" } | { kind: "shown" };

export function backupsDoor(input: {
  service: ServiceInfo;
  gone: boolean;
  /** L'agent annonce `service-backups` (`ServiceRights.serviceBackups`). */
  supported: boolean;
}): BackupsDoor {
  const { service, gone, supported } = input;
  if (gone || !supported) return { kind: "hidden" };
  if (service.state === "deploying") return { kind: "hidden" };
  if (service.adopted !== null || service.origin !== "hearth") return { kind: "hidden" };
  return { kind: "shown" };
}

/**
 * Ce que les boutons d'écriture (« Sauvegarder », « Éléments ignorés », « Supprimer ») montrent :
 * - `ready` : actifs ;
 * - `blocked` : visibles et désactivés ; `rights` : c'est le droit (sa raison s'écrit par `ServiceRightReason`, une seule
 *   fois), sinon `reason` est la phrase de raison, avec `by` quand une action d'un autre compte tient le service.
 */
export type WriteGate =
  | { kind: "ready" }
  | { kind: "blocked"; rights: boolean; reason: string | null; by: string | null };

export function writeGate(input: {
  gate: RightsGate;
  lock: PilotLock;
  /** Le moteur est prêt : seule la création en a besoin ; la liste, la vérification et la suppression ne demandent que le lien. */
  engineReady: boolean;
  /** L'état du service ne permet pas de sauvegarder (`null` pour ne pas juger : suppression, vérification). */
  state: ServiceState | null;
}): WriteGate {
  const { gate, lock, engineReady, state } = input;
  if (!canAct(gate)) return { kind: "blocked", rights: true, reason: null, by: null };
  if (lock.locked) {
    return { kind: "blocked", rights: false, reason: busyText(lock), by: lock.by };
  }
  if (state !== null && !SAVABLE.includes(state)) {
    const transition = state === "starting" || state === "stopping" || state === "updating";
    return {
      kind: "blocked",
      rights: false,
      reason: t(transition ? "backups.busyBlock" : "backups.notDrivable"),
      by: null,
    };
  }
  if (state !== null && !engineReady) {
    return { kind: "blocked", rights: false, reason: t("backups.engineNotReady"), by: null };
  }
  return { kind: "ready" };
}

/** La raison d'un blocage, avec « Lancée par {compte}. » quand l'agent dit qui. */
export function blockedReason(gate: WriteGate): string | null {
  if (gate.kind !== "blocked" || gate.rights || gate.reason === null) return null;
  return gate.by === null ? gate.reason : `${gate.reason} ${gate.by}`;
}

// ── La liste ─────────────────────────────────────────────────────────────────────────────────────

/** Une date d'une sauvegarde, dans le fuseau du poste (jamais le texte brut de l'agent). */
export function dateOf(iso: string): string {
  return formatDateTime(iso);
}

const MODE_KEYS: Record<BackupMode, MessageKey> = {
  stopped: "backups.mode.stopped",
  hot: "backups.mode.hot",
  stop_copy_start: "backups.mode.stopCopyStart",
  unknown: "backups.mode.unknown",
};

export function modeLabel(mode: BackupMode): string {
  return t(MODE_KEYS[mode]);
}

const FLAG_KEYS: Partial<Record<BackupFlag, MessageKey>> = {
  hot: "backups.flag.hot",
  maybe_inconsistent: "backups.flag.maybeInconsistent",
  forced_stop: "backups.flag.forcedStop",
  before_restore: "backups.flag.beforeRestore",
};

/** Les repères d'une sauvegarde en texte ; un repère que ce client ne connaît pas est écarté, jamais affiché tel quel. */
export function flagLabels(backup: Pick<BackupInfo, "flags">): string[] {
  const seen = new Set<BackupFlag>();
  const labels: string[] = [];
  for (const flag of backup.flags) {
    const key = FLAG_KEYS[flag];
    if (!key || seen.has(flag)) continue;
    seen.add(flag);
    labels.push(t(key));
  }
  return labels;
}

/** Le repère est une alerte (peut être incohérente, arrêt forcé) : il prend le ton d'avertissement. */
export function flagIsWarning(label: string): boolean {
  return label === t("backups.flag.maybeInconsistent") || label === t("backups.flag.forcedStop");
}

/** L'état d'intégrité en texte : « Vérifiée le {date} », « Corrompue »… (BR-BACKUP-013). */
export function integrityText(backup: Pick<BackupInfo, "integrity" | "verifiedAt">): string {
  switch (backup.integrity) {
    case "verified":
      return backup.verifiedAt
        ? t("backups.integrity.verified", { date: dateOf(backup.verifiedAt) })
        : t("backups.integrity.verified", { date: "—" });
    case "corrupt":
      return t("backups.integrity.corrupt");
    case "unchecked":
      return t("backups.integrity.unchecked");
    case "missing":
      return t("backups.integrity.missing");
    default:
      return t("backups.integrity.unknown");
  }
}

/** « 1,2 Go · 48 210 fichiers » : la taille des données et le nombre de fichiers (BR-BACKUP-012). */
export function dataLine(backup: Pick<BackupInfo, "dataBytes" | "files">): string {
  return backup.files === 1
    ? t("backups.sizeDetailOne", { data: formatSize(backup.dataBytes) })
    : t("backups.sizeDetail", {
        data: formatSize(backup.dataBytes),
        count: formatCount(backup.files),
      });
}

/** La note, en texte inoffensif ; `null` quand il n'y en a pas. */
export function noteOf(backup: Pick<BackupInfo, "note">): string | null {
  if (backup.note === null || backup.note === "") return null;
  return safeText(backup.note);
}

/** Le compte qui a lancé la sauvegarde ; un identifiant qui n'a pas la forme d'un compte n'est jamais affiché. */
export function accountOf(backup: Pick<BackupInfo, "account">): string {
  return isHearthUsername(backup.account) ? backup.account : t("backups.unknownAccount");
}

/** L'image et la version du service au moment de la sauvegarde. */
export function imageOf(backup: Pick<BackupInfo, "image">): string {
  return backup.image === "" ? t("backups.noImage") : safeText(backup.image);
}

/** Le nom accessible d'une ligne : « Sauvegarde du {date}, {taille}, {mode}, {état}{, repères} ». */
export function rowLabel(backup: BackupInfo): string {
  const flags = flagLabels(backup);
  const params = {
    date: dateOf(backup.createdAt),
    size: formatSize(backup.archiveBytes),
    mode: modeLabel(backup.mode),
    state: integrityText(backup),
  };
  return flags.length === 0
    ? t("backups.rowLabel", params)
    : t("backups.rowLabelFlags", { ...params, flags: flags.join(", ") });
}

/** « 18,2 Go pour 6 sauvegardes » (BR-BACKUP-015). */
export function spaceLine(view: Pick<BackupsView, "backups" | "totalBytes">): string {
  const size = formatSize(view.totalBytes);
  return view.backups.length === 1
    ? t("backups.spaceLineOne", { size })
    : t("backups.spaceLine", { size, count: view.backups.length });
}

/** Les avertissements permanents de la carte « Espace » (BR-BACKUP-005, 015) : jamais repliés. */
export interface SpaceWarning {
  key: string;
  text: string;
}

export function spaceWarnings(view: BackupsView): SpaceWarning[] {
  const warnings: SpaceWarning[] = [];
  if (view.sameDisk) warnings.push({ key: "same-disk", text: t("backups.sameDisk") });
  warnings.push({ key: "content", text: t("backups.contentWarning") });
  if (view.backups.length > WARN_ABOVE) {
    warnings.push({
      key: "many",
      text: t("backups.many", {
        count: view.backups.length,
        size: formatSize(view.totalBytes),
      }),
    });
  }
  if (view.freeBytes !== null && view.freeBytes < ALMOST_FULL_BYTES) {
    warnings.push({
      key: "almost-full",
      text: t("backups.diskAlmostFull", { free: formatSize(view.freeBytes) }),
    });
  }
  return warnings;
}

/** Il n'y a plus une seule sauvegarde vérifiée en dehors de celle-ci (BR-BACKUP-027 : « C'est la seule… »). */
export function isOnlyVerified(backups: readonly BackupInfo[], target: BackupInfo): boolean {
  if (target.integrity !== "verified") return false;
  return backups.filter((backup) => backup.integrity === "verified").length === 1;
}

/** « {chemin dans le service} » : la racine et le chemin joints par une barre, jamais deux barres de suite. */
export function itemPath(item: IgnoredItem): string {
  const root = item.root.replace(/\/+$/, "");
  const path = item.path.replace(/^\/+/, "");
  const joined = path === "" ? root : `${root}/${path}`;
  return safeText(joined === "" ? "/" : joined);
}

// ── Les éléments ignorés ──────────────────────────────────────────────────────────────────────────

/** Les mêmes éléments (racine et chemin). */
export function sameItem(a: IgnoredItem, b: IgnoredItem): boolean {
  return a.root === b.root && a.path === b.path;
}

/**
 * Lit ce que le compte a tapé : un chemin DANS LE SERVICE, au plus profond des dossiers de données connus. `null` :
 * le chemin n'est dans aucun dossier de données (BR-BACKUP-003). Les `..` et les chemins relatifs sont refusés ici,
 * l'agent juge ensuite.
 */
export function parseItem(typed: string, roots: readonly string[]): IgnoredItem | null {
  const text = typed.trim();
  if (!text.startsWith("/") || text.includes("\0")) return null;
  const parts = text.split("/").filter((part) => part !== "");
  if (parts.some((part) => part === "." || part === "..")) return null;
  const normal = `/${parts.join("/")}`;
  let best: string | null = null;
  for (const root of roots) {
    const base = root.replace(/\/+$/, "") || "/";
    const inside = base === "/" || normal === base || normal.startsWith(`${base}/`);
    if (inside && (best === null || base.length > best.length)) best = base;
  }
  if (best === null) return null;
  const path = best === "/" ? normal.slice(1) : normal.slice(best.length + 1);
  return { root: best, path };
}

export type AddCheck = "ok" | "outside" | "already" | "too-many" | "empty";

/** Peut-on ajouter ce que le compte a tapé à la liste ? */
export function checkAdd(
  typed: string,
  roots: readonly string[],
  current: readonly IgnoredItem[],
): { kind: AddCheck; item: IgnoredItem | null } {
  if (typed.trim() === "") return { kind: "empty", item: null };
  const item = parseItem(typed, roots);
  if (!item) return { kind: "outside", item: null };
  if (current.some((existing) => sameItem(existing, item))) return { kind: "already", item };
  if (current.length >= MAX_ITEMS) return { kind: "too-many", item };
  return { kind: "ok", item };
}

export function addErrorText(kind: AddCheck): string | undefined {
  switch (kind) {
    case "outside":
      return t("backups.addOutside");
    case "already":
      return t("backups.addAlready");
    case "too-many":
      return t("backups.addTooMany", { n: MAX_ITEMS });
    default:
      return undefined;
  }
}

/** Les dossiers de données d'un service : les cibles de ses montages de dossier (BR-BACKUP-002). */
export function dataRoots(service: Pick<ServiceInfo, "mounts">): string[] {
  const roots = new Set<string>();
  for (const mount of service.mounts) {
    if (mount.kind === "folder" && mount.target.startsWith("/")) roots.add(mount.target);
  }
  return [...roots];
}

// ── La fenêtre de création ────────────────────────────────────────────────────────────────────────

/** Le mode demandé quand le service ne tourne pas : toujours une copie directe (BR-BACKUP-006). */
export function defaultMode(preview: Pick<BackupPreview, "running">): BackupMode {
  return preview.running ? "hot" : "stopped";
}

/** Les cartes de mode ne se montrent que pour un service en marche. */
export function offersModes(preview: Pick<BackupPreview, "running">): boolean {
  return preview.running;
}

export function graceText(preview: Pick<BackupPreview, "stopGraceS">): string {
  if (preview.stopGraceS <= 0) return t("backups.modeStopGraceNone");
  const seconds = preview.stopGraceS;
  const delay =
    seconds >= 120 && seconds % 60 === 0 ? `${seconds / 60} minutes` : `${seconds} secondes`;
  return t("backups.modeStopGrace", { delay });
}

export function noteLength(note: string): number {
  return [...note].length;
}

export function noteTooLong(note: string): boolean {
  return noteLength(note) > NOTE_MAX;
}

/** Il faut jusqu'à {x} ; il reste {y} (réserve de {r} comprise) : le refus chiffré, sans case pour passer outre (BR-BACKUP-009). */
export function noSpaceText(input: {
  needed: number | null;
  free: number | null;
  reserve: number | null;
}): string {
  const needed = input.needed === null ? "—" : formatSize(input.needed);
  const reserve = input.reserve === null ? "—" : formatSize(input.reserve);
  if (input.free === null) return t("backups.noSpaceNoFree", { needed, reserve });
  return t("backups.noSpace", { needed, free: formatSize(input.free), reserve });
}

/** La fenêtre dit en tête que la place manque : l'estimation d'avant l'envoi (BR-BACKUP-009). */
export function previewNoSpace(preview: BackupPreview): string | null {
  if (preview.enoughSpace) return null;
  return noSpaceText({
    needed: preview.bytes,
    free: preview.freeBytes,
    reserve: preview.reserveBytes,
  });
}

export function estimateText(preview: Pick<BackupPreview, "bytes" | "files" | "partial">): string {
  return t(preview.partial ? "backups.estimateMore" : "backups.estimate", {
    size: formatSize(preview.bytes),
    count: formatCount(preview.files),
  });
}

export function bigFileText(file: BigFile): string {
  return t("backups.bigFile", { path: itemPath(file), size: formatSize(file.bytes) });
}

/** La demande envoyée : le mode (un service arrêté ne choisit pas), la note nettoyée et les gros fichiers cochés. */
export function requestOf(input: {
  preview: Pick<BackupPreview, "running">;
  mode: BackupMode;
  note: string;
  ignored: readonly IgnoredItem[];
}): BackupRequest {
  const note = input.note.trim();
  return {
    mode: input.preview.running ? input.mode : "stopped",
    note: note === "" ? null : note,
    ignoreOnce: input.ignored.map((item) => ({ root: item.root, path: item.path })),
  };
}

// ── Les refus ─────────────────────────────────────────────────────────────────────────────────────

function engineText(engine: EngineState | null): string {
  if (engine === null || engine === "checking") return t("backups.refused.engineNoState");
  const key = ENGINE_STATE_KEYS[engine];
  return key
    ? t("backups.refused.engine", { state: t(key).toLowerCase() })
    : t("backups.refused.engineNoState");
}

function pathText(reason: string | null): string {
  return reason === "backups_root_overlap"
    ? t("backups.refused.overlap")
    : t("backups.refused.path");
}

/** Le texte d'un refus de lecture (liste, estimation, vérification), jamais un texte de l'agent. */
export function readRefusalText(refusal: BackupReadRefusal): string {
  switch (refusal.kind) {
    case "service_not_found":
      return t("backups.readRefused.serviceNotFound");
    case "service_adopted":
      return t("backups.readRefused.adopted");
    case "engine_unavailable":
      return t("backups.readRefused.engine");
    case "path_refused":
      return refusal.reason === "backups_root_overlap"
        ? t("backups.readRefused.overlap")
        : t("backups.readRefused.path");
    case "backup_not_found":
      return t("backups.refused.notFound");
    case "backup_in_use":
      return t("backups.refused.inUse");
    default:
      return t("backups.readRefused.other");
  }
}

/**
 * Le texte d'un refus d'acte qui n'est pas celui de la confirmation (le mot de passe se dit sous son champ) ; `undefined`
 * quand le refus se dit ailleurs. Un code que ce client ne connaît pas devient le refus générique.
 */
export function refusalText(refusal: { kind: string; retry_after_s?: number }): string | undefined {
  const known = refusal as BackupRefusal;
  switch (known.kind) {
    case "service_not_found":
      return t("backups.refused.gone");
    case "service_adopted":
      return t("backups.refused.adopted");
    case "engine_unavailable":
      return engineText(known.engine);
    case "service_busy":
      return known.by !== null && isHearthUsername(known.by)
        ? t("backups.refused.busyBy", { account: known.by })
        : t("backups.refused.busy");
    case "heavy_io_busy":
      return t("backups.refused.heavy");
    case "not_drivable":
      return t("backups.refused.notDrivable");
    case "no_space":
      return noSpaceText({
        needed: known.needed_bytes,
        free: known.free_bytes,
        reserve: known.reserve_bytes,
      });
    case "nothing_to_save":
      return t("backups.refused.nothingToSave");
    case "invalid_input":
      return known.field === "note" ? t("backups.refused.note") : t("backups.refused.items");
    case "mode_refused":
      return t("backups.refused.mode");
    case "path_refused":
      return pathText(known.reason);
    case "backup_not_found":
      return t("backups.refused.notFound");
    case "backup_in_use":
      return t("backups.refused.inUse");
    default:
      return t("backups.refused.other");
  }
}
