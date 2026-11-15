import type {
  BackupViewDto,
  BackupCreateOutcome as BoundCreateOutcome,
  BackupDeleteOutcome as BoundDeleteOutcome,
  BackupsOutcome as BoundListOutcome,
  BackupPreviewOutcome as BoundPreviewOutcome,
  BackupSettingsOutcome as BoundSettingsOutcome,
  BackupVerifyOutcome as BoundVerifyOutcome,
} from "@/bindings";
import type { AdminActKind } from "./reauth";
import type { OperationKind } from "./serviceOps";
import type { EngineState, ServiceState } from "./services";

/**
 * Sauvegarder les données d'un service (HRT-98, HRT-97, `BR-BACKUP-*`) : types de l'interface et conversions depuis les
 * types générés (`bindings.ts`). Aucune règle ici : la liaison construit l'acte, l'agent arbitre. Une réponse `202` ne
 * dit JAMAIS l'issue : elle arrive par le suivi des opérations (HRT-84). Les chemins sont ceux du SERVICE (cible de
 * montage), jamais ceux de la machine ; la note d'un compte est un texte à montrer tel quel (jamais du HTML) et ne
 * s'écrit dans aucune trace.
 */

/** Les trois actes d'administration des sauvegardes. Supprimer n'est jamais couvert par l'élévation (BR-BACKUP-027). */
export const BACKUP_CREATE_ACT_KIND: AdminActKind = "backup_create";
export const BACKUP_DELETE_ACT_KIND: AdminActKind = "backup_delete";
export const BACKUP_SETTINGS_ACT_KIND: AdminActKind = "backup_settings";

/** Comment la copie est faite (BR-BACKUP-006). */
export type BackupMode = "stopped" | "hot" | "stop_copy_start" | "unknown";

/** Un repère d'une sauvegarde (BR-BACKUP-012). */
export type BackupFlag =
  | "hot"
  | "maybe_inconsistent"
  | "forced_stop"
  | "before_restore"
  | "unknown";

/** L'état d'intégrité d'une sauvegarde (BR-BACKUP-013). */
export type BackupIntegrity = "verified" | "corrupt" | "unchecked" | "missing" | "unknown";

/** Pourquoi un élément est ignoré (BR-BACKUP-003). */
export type IgnoredWhy = "looks_like_backups" | "chosen" | "this_time" | "unknown";

/** Un dossier du service (par sa cible) et un chemin dans ce dossier (`/` pour séparateur ; vide : le dossier entier). */
export interface IgnoredItem {
  root: string;
  path: string;
}

export interface IgnoredView extends IgnoredItem {
  why: IgnoredWhy;
}

/** Les éléments ignorés d'un service : ce que le compte ajoute, et ce qu'il retire des ignorés par défaut. */
export interface BackupSettings {
  ignored: IgnoredItem[];
  included: IgnoredItem[];
}

/** Une sauvegarde de la liste. */
export interface BackupInfo {
  id: string;
  /** Date de création, RFC 3339 en UTC. */
  createdAt: string;
  archiveBytes: number;
  dataBytes: number;
  files: number;
  mode: BackupMode;
  flags: BackupFlag[];
  changedFiles: number;
  /** Éléments que la copie n'a pas archivés : dits, jamais omis en silence. */
  skipped: number;
  note: string | null;
  /** Le compte qui l'a demandée (identifiant technique). */
  account: string;
  /** `dépôt:version` de l'image du service au moment de la sauvegarde ; vide : inconnue. */
  image: string;
  integrity: BackupIntegrity;
  verifiedAt: string | null;
}

/** La liste des sauvegardes d'un service, l'espace pris, l'espace libre et les éléments ignorés. */
export interface BackupsView {
  /** Les plus récentes d'abord. */
  backups: BackupInfo[];
  totalBytes: number;
  freeBytes: number | null;
  /** La racine des sauvegardes est sur le même disque que des données sauvegardées (BR-BACKUP-005). */
  sameDisk: boolean;
  settings: BackupSettings;
  /** Le service n'a aucun dossier de données sauvegardable (BR-BACKUP-002). */
  nothingToSave: boolean;
}

/** Un gros fichier signalé à la fenêtre de création (BR-BACKUP-033). */
export interface BigFile extends IgnoredItem {
  bytes: number;
}

/** De quoi bâtir la fenêtre de création. */
export interface BackupPreview {
  name: string;
  running: boolean;
  /** Le délai d'arrêt propre du service, en secondes (BR-BACKUP-008). */
  stopGraceS: number;
  /** Les dossiers du service qui seront sauvegardés (cible dans le service). */
  roots: string[];
  /** Les dossiers du service qui ne le sont pas (BR-BACKUP-028). */
  notSaved: string[];
  bytes: number;
  files: number;
  /** L'estimation a été coupée : « au moins ». */
  partial: boolean;
  bigFiles: BigFile[];
  ignored: IgnoredView[];
  ignoredMore: number;
  freeBytes: number | null;
  reserveBytes: number;
  /** Faux : aucune case ne permet de passer outre (BR-BACKUP-009). */
  enoughSpace: boolean;
  sameDisk: boolean;
  nothingToSave: boolean;
}

/** Pourquoi l'agent n'a pas donné une lecture (liste, estimation, vérification). */
export type BackupReadRefusal =
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "path_refused"; reason: string | null }
  | { kind: "backup_not_found" }
  | { kind: "backup_in_use" }
  | { kind: "other" };

export type BackupsOutcome =
  | { kind: "ready"; view: BackupsView }
  | { kind: "refused"; refusal: BackupReadRefusal };

export type BackupPreviewOutcome =
  | { kind: "ready"; preview: BackupPreview }
  | { kind: "refused"; refusal: BackupReadRefusal };

/** Le résultat d'une vérification (BR-BACKUP-013). */
export interface VerifyResult {
  integrity: BackupIntegrity;
  verifiedAt: string | null;
  filesChecked: number;
}

export type BackupVerifyOutcome =
  | { kind: "done"; result: VerifyResult }
  | { kind: "refused"; refusal: BackupReadRefusal };

/** La demande de création : le mode, la note facultative et les éléments ignorés pour cette sauvegarde seulement. */
export interface BackupRequest {
  mode: BackupMode;
  note: string | null;
  ignoreOnce: IgnoredItem[];
}

/** Pourquoi l'agent a refusé un acte de sauvegarde (liste fermée de codes ; l'interface les met en phrases). */
export type BackupRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "service_busy"; by: string | null; action: OperationKind | null }
  | { kind: "heavy_io_busy" }
  | { kind: "not_drivable"; state: ServiceState | null }
  | {
      kind: "no_space";
      needed_bytes: number | null;
      free_bytes: number | null;
      reserve_bytes: number | null;
    }
  | { kind: "nothing_to_save" }
  | { kind: "invalid_input"; field: string | null }
  | { kind: "path_refused"; reason: string | null }
  | { kind: "backup_not_found" }
  | { kind: "backup_in_use" }
  | { kind: "mode_refused" }
  | { kind: "other" };

/**
 * L'issue de la création : acceptée (l'opération court à l'arrière-plan), refusée, ou résultat inconnu (le lien est
 * tombé avant la réponse : jamais rejoué).
 */
export type BackupCreateOutcome =
  | { kind: "accepted"; operationId: string; backupId: string }
  | { kind: "refused"; refusal: BackupRefusal }
  | { kind: "unknown"; opId: string };

export type BackupDeleteOutcome =
  | { kind: "done"; removedFiles: boolean }
  | { kind: "refused"; refusal: BackupRefusal }
  | { kind: "unknown"; opId: string };

export type BackupSettingsOutcome =
  | { kind: "done"; settings: BackupSettings }
  | { kind: "refused"; refusal: BackupRefusal }
  | { kind: "unknown"; opId: string };

/** Un nombre que le fil ne sait pas porter (`null`) n'est jamais lu comme un zéro mesuré. */
const num = (value: number | null | undefined): number => value ?? 0;

function toInfo(dto: BackupViewDto): BackupInfo {
  return {
    id: dto.id,
    createdAt: dto.createdAt,
    archiveBytes: num(dto.archiveBytes),
    dataBytes: num(dto.dataBytes),
    files: num(dto.files),
    mode: dto.mode,
    flags: dto.flags,
    changedFiles: dto.changedFiles,
    skipped: dto.skipped,
    note: dto.note,
    account: dto.account,
    image: dto.image,
    integrity: dto.integrity,
    verifiedAt: dto.verifiedAt,
  };
}

export function toBackupsOutcome(dto: BoundListOutcome): BackupsOutcome {
  if (dto.kind === "refused") {
    return { kind: "refused", refusal: dto.refusal as BackupReadRefusal };
  }
  const view = dto.view;
  return {
    kind: "ready",
    view: {
      backups: view.backups.map(toInfo),
      totalBytes: num(view.totalBytes),
      freeBytes: view.freeBytes,
      sameDisk: view.sameDisk,
      settings: view.settings,
      nothingToSave: view.nothingToSave,
    },
  };
}

export function toBackupPreviewOutcome(dto: BoundPreviewOutcome): BackupPreviewOutcome {
  if (dto.kind === "refused") {
    return { kind: "refused", refusal: dto.refusal as BackupReadRefusal };
  }
  const preview = dto.preview;
  return {
    kind: "ready",
    preview: {
      name: preview.name,
      running: preview.running,
      stopGraceS: preview.stopGraceS,
      roots: preview.roots,
      notSaved: preview.notSaved,
      bytes: num(preview.bytes),
      files: num(preview.files),
      partial: preview.partial,
      bigFiles: preview.bigFiles.map((file) => ({ ...file, bytes: num(file.bytes) })),
      ignored: preview.ignored,
      ignoredMore: preview.ignoredMore,
      freeBytes: preview.freeBytes,
      reserveBytes: num(preview.reserveBytes),
      enoughSpace: preview.enoughSpace,
      sameDisk: preview.sameDisk,
      nothingToSave: preview.nothingToSave,
    },
  };
}

export function toBackupVerifyOutcome(dto: BoundVerifyOutcome): BackupVerifyOutcome {
  if (dto.kind === "refused") {
    return { kind: "refused", refusal: dto.refusal as BackupReadRefusal };
  }
  return {
    kind: "done",
    result: {
      integrity: dto.result.integrity,
      verifiedAt: dto.result.verifiedAt,
      filesChecked: num(dto.result.filesChecked),
    },
  };
}

export function toBackupCreateOutcome(dto: BoundCreateOutcome): BackupCreateOutcome {
  switch (dto.kind) {
    case "accepted":
      return { kind: "accepted", operationId: dto.operationId, backupId: dto.backupId };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as BackupRefusal };
  }
}

export function toBackupDeleteOutcome(dto: BoundDeleteOutcome): BackupDeleteOutcome {
  switch (dto.kind) {
    case "done":
      return { kind: "done", removedFiles: dto.removedFiles };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as BackupRefusal };
  }
}

export function toBackupSettingsOutcome(dto: BoundSettingsOutcome): BackupSettingsOutcome {
  switch (dto.kind) {
    case "done":
      return { kind: "done", settings: dto.settings };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as BackupRefusal };
  }
}
