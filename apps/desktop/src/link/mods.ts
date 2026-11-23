import type {
  ModsApplyOutcome as BoundApplyOutcome,
  BackupModsOutcome as BoundBackupModsOutcome,
  ModsCheckOutcome as BoundCheckOutcome,
  ModsOutcome as BoundModsOutcome,
  OperationTailOutcome as BoundTailOutcome,
  ModIdentityDto,
} from "@/bindings";
import type { AdminActKind } from "./reauth";
import type { BackupRefusal } from "./serviceBackups";

/**
 * Les mods d'un serveur Minecraft installé (HRT-260, ADR-0062, `BR-MOD-020` à `036`) : types de l'interface et
 * conversions depuis les types générés (`bindings.ts`). Aucune règle ici : le jugement (compatibilité, doublons,
 * dépendances) est celui de l'agent, l'interface met en phrases. L'application d'un lot est un acte d'administration
 * JAMAIS couvert par l'élévation ; une réponse `202` ne dit jamais l'issue : elle arrive par le suivi des opérations.
 * Les noms de fichiers et de mods sont du texte (jamais du HTML).
 */

export const MODS_APPLY_ACT_KIND: AdminActKind = "mods_apply";

/** Ce que le serveur fait des mods (BR-MOD-030). */
export type ModsApplicable = "mods" | "no_mods" | "plugins" | "no_way" | "unknown";
export type MinecraftFrom = "setting" | "log" | "modrinth_manifest" | "unknown";
export type ModsPackKind = "none" | "server_pack" | "modrinth_pack" | "unknown";
export type ModPlace = "active" | "retired" | "unknown";
/** L'origine d'un mod, en quatre valeurs (BR-MOD-033). */
export type ModOrigin = "server_pack" | "hearth" | "modrinth_pack" | "manual" | "unknown";
export type ModEnvironment = "both" | "server" | "client" | "unknown";
export type VerdictCode =
  | "compatible"
  | "unverified"
  | "to_complete"
  | "incompatible_loader"
  | "incompatible_minecraft"
  | "duplicate"
  | "unrecognized"
  | "unknown";

const APPLICABLE: readonly ModsApplicable[] = ["mods", "no_mods", "plugins", "no_way"];
const MINECRAFT_FROM: readonly MinecraftFrom[] = ["setting", "log", "modrinth_manifest"];
const PACK_KIND: readonly ModsPackKind[] = ["none", "server_pack", "modrinth_pack"];
const PLACE: readonly ModPlace[] = ["active", "retired"];
const ORIGIN: readonly ModOrigin[] = ["server_pack", "hearth", "modrinth_pack", "manual"];
const ENVIRONMENT: readonly ModEnvironment[] = ["both", "server", "client"];
const VERDICT: readonly VerdictCode[] = [
  "compatible",
  "unverified",
  "to_complete",
  "incompatible_loader",
  "incompatible_minecraft",
  "duplicate",
  "unrecognized",
];

/** Un mot du fil lu contre une liste fermée : toute valeur inconnue devient `unknown`, jamais une permission. */
export function oneOf<T extends string>(list: readonly T[], value: string): T | "unknown" {
  return (list as readonly string[]).includes(value) ? (value as T) : "unknown";
}

/** Ce que le mod dit de lui-même. */
export interface ModIdentity {
  id: string;
  name: string | null;
  version: string | null;
  loader: string;
  /** La plage de Minecraft du mod est comprise par l'agent. */
  minecraftDeclared: boolean;
  environment: ModEnvironment;
}

export interface ModVerdict {
  code: VerdictCode;
  /** Les identifiants des mods manquants (`to_complete`). */
  missing: string[];
  /** Les identifiants des mods qui ont besoin de celui-ci. */
  neededBy: string[];
}

/** Un fichier du dossier des mods ou de « Mods retirés ». */
export interface ModItem {
  /** Absent : nom hors de l'alphabet sûr, le fichier se montre par son rang et ne se manipule pas. */
  file: string | null;
  rank: number;
  place: ModPlace;
  /** Faux pour un `.jar.disabled`. */
  enabled: boolean;
  origin: ModOrigin;
  readableName: boolean;
  identity: ModIdentity | null;
  verdict: ModVerdict;
}

export interface ModsGame {
  loader: string | null;
  minecraft: string | null;
  minecraftFrom: MinecraftFrom;
  /** Le serveur prend la dernière version à chaque démarrage (BR-MOD-028). */
  updatesEveryStart: boolean;
  pack: ModsPackKind;
}

export interface ModsView {
  applicable: ModsApplicable;
  game: ModsGame;
  mods: ModItem[];
  /** Jars pas encore lus : l'interface relit (BR-MOD-020). */
  unread: number;
  truncated: boolean;
}

/** Pourquoi l'agent n'a pas donné une lecture des mods. */
export type ModsReadRefusal =
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "not_applicable" }
  | { kind: "deposit_unknown" }
  | { kind: "folder_blocked" }
  | { kind: "backup_not_found" }
  | { kind: "other" };

export type ModsOutcome =
  | { kind: "ready"; view: ModsView }
  | { kind: "refused"; refusal: ModsReadRefusal };

/** L'issue d'un dépôt jugé (BR-MOD-023). */
export type CheckOutcome = "add" | "replace" | "refused" | "unknown";

export interface ModCheckFile {
  deposit: string;
  outcome: CheckOutcome;
  identity: ModIdentity | null;
  /** Le nom sous lequel Hearth poserait le mod (choisi par Hearth). */
  targetFile: string | null;
  replaces: { file: string; version: string | null } | null;
  verdict: ModVerdict | null;
  /** Un code `MOD_*` pour un dépôt refusé. */
  refusal: string | null;
  refusalValues: {
    loader: string | null;
    serverLoader: string | null;
    serverMinecraft: string | null;
  } | null;
}

export type ModsCheckOutcome =
  | { kind: "ready"; files: ModCheckFile[] }
  | { kind: "refused"; refusal: ModsReadRefusal };

/** Un ajout du lot : le dépôt contrôlé, l'empreinte lue au contrôle, et le fichier remplacé. */
export interface ModAdd {
  deposit: string;
  sha256: string;
  replace: string | null;
}

/** Le lot à appliquer, tel que l'agent le reçoit : au moins un changement, 64 au plus, 8 ajouts au plus. */
export interface ModsBatch {
  add: ModAdd[];
  retire: string[];
  restore: string[];
  disable: string[];
  enable: string[];
}

export const MAX_BATCH_CHANGES = 64;
export const MAX_BATCH_ADDS = 8;

/** Le nombre de changements d'un lot. */
export function batchSize(batch: ModsBatch): number {
  return (
    batch.add.length +
    batch.retire.length +
    batch.restore.length +
    batch.disable.length +
    batch.enable.length
  );
}

/** Pourquoi l'agent a refusé l'application d'un lot. */
export type ModsApplyRefusal =
  /** Mot de passe, verrou, espace, moteur : comme une sauvegarde. */
  | { kind: "common"; refusal: BackupRefusal }
  /** Un refus propre aux mods : `MOD_REFUSED`, `MOD_FILE_MISSING`, `MOD_IN_MODRINTH_PACK`, `MOD_ALREADY_ACTIVE`, etc. */
  | { kind: "mod"; code: string; deposit: string | null; reason: string | null };

export type ModsApplyOutcome =
  | { kind: "accepted"; operationId: string; backupId: string }
  | { kind: "refused"; refusal: ModsApplyRefusal }
  | { kind: "unknown"; opId: string };

/** Les dernières lignes du journal d'une opération et, quand l'agent la reconnaît, la cause en clair (BR-MOD-027). */
export interface OperationTail {
  lines: string[];
  /** `duplicate_mod`, `missing_dependency`, `incompatible`, `out_of_memory` : absent si non reconnue. */
  reason: string | null;
  sentence: string | null;
}

export type OperationTailOutcome =
  | { kind: "ready"; tail: OperationTail }
  | { kind: "refused"; refusal: ModsReadRefusal };

// ── Les mods d'une sauvegarde (HRT-294, BR-MOD-034 à 036) ─────────────────────────────────────────────────────────────

export type BackupModsState =
  | "recorded"
  | "partial"
  | "unreadable"
  | "not_in_backup"
  | "not_recorded"
  | "unknown";
export type BackupModPlace = "active" | "disabled" | "retired" | "unknown";
export type ChangeKind =
  | "added"
  | "removed"
  | "version_changed"
  | "enabled"
  | "disabled"
  | "unknown";

const BACKUP_STATE: readonly BackupModsState[] = [
  "recorded",
  "partial",
  "unreadable",
  "not_in_backup",
  "not_recorded",
];
const BACKUP_PLACE: readonly BackupModPlace[] = ["active", "disabled", "retired"];
const CHANGE_KIND: readonly ChangeKind[] = [
  "added",
  "removed",
  "version_changed",
  "enabled",
  "disabled",
];

export interface BackupModItem {
  file: string | null;
  rank: number;
  place: BackupModPlace;
  origin: ModOrigin;
  /** Absente : « Non reconnu dans cette sauvegarde ». */
  identity: ModIdentity | null;
}

export interface ModChange {
  kind: ChangeKind;
  id: string | null;
  name: string | null;
  file: string | null;
  backupVersion: string | null;
  currentVersion: string | null;
}

export interface BackupModsCompare {
  /** Faux tant que la liste actuelle a des jars non lus : l'interface relit. */
  complete: boolean;
  unread: number;
  same: boolean;
  changes: ModChange[];
}

export interface BackupMods {
  state: BackupModsState;
  recordedAt: string | null;
  game: { loader: string | null; minecraft: string | null };
  counts: { active: number; disabled: number; retired: number; notListed: number };
  mods: BackupModItem[];
  compare: BackupModsCompare | null;
}

export type BackupModsOutcome =
  | { kind: "ready"; view: BackupMods }
  | { kind: "refused"; refusal: ModsReadRefusal };

/** Le résumé joint à chaque sauvegarde de la liste (colonne « Mods »). */
export interface BackupModsSummary {
  state: BackupModsState;
  active: number;
  disabled: number;
  retired: number;
}

export function toBackupModsState(value: string): BackupModsState {
  return oneOf(BACKUP_STATE, value);
}

// ── Conversions ─────────────────────────────────────────────────────────────────────────────────────────────────────

function toIdentity(dto: ModIdentityDto): ModIdentity {
  return {
    id: dto.id,
    name: dto.name,
    version: dto.version,
    loader: dto.loader,
    minecraftDeclared: dto.minecraftDeclared,
    environment: oneOf(ENVIRONMENT, dto.environment),
  };
}

function toVerdict(dto: { code: string; missing: string[]; neededBy: string[] }): ModVerdict {
  return { code: oneOf(VERDICT, dto.code), missing: dto.missing, neededBy: dto.neededBy };
}

function toRefusal(refusal: { kind: string }): ModsReadRefusal {
  return refusal as ModsReadRefusal;
}

export function toModsOutcome(dto: BoundModsOutcome): ModsOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: toRefusal(dto.refusal) };
  const view = dto.view;
  return {
    kind: "ready",
    view: {
      applicable: oneOf(APPLICABLE, view.applicable),
      game: {
        loader: view.game.loader,
        minecraft: view.game.minecraft,
        minecraftFrom: oneOf(MINECRAFT_FROM, view.game.minecraftFrom),
        updatesEveryStart: view.game.updatesEveryStart,
        pack: oneOf(PACK_KIND, view.game.pack),
      },
      mods: view.mods.map((item) => ({
        file: item.file,
        rank: item.rank,
        place: oneOf(PLACE, item.place),
        enabled: item.enabled,
        origin: oneOf(ORIGIN, item.origin),
        readableName: item.readableName,
        identity: item.identity ? toIdentity(item.identity) : null,
        verdict: toVerdict(item.verdict),
      })),
      unread: view.unread,
      truncated: view.truncated,
    },
  };
}

export function toModsCheckOutcome(dto: BoundCheckOutcome): ModsCheckOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: toRefusal(dto.refusal) };
  return {
    kind: "ready",
    files: dto.view.files.map((file) => ({
      deposit: file.deposit,
      outcome: oneOf(["add", "replace", "refused"] as const, file.outcome),
      identity: file.identity ? toIdentity(file.identity) : null,
      targetFile: file.targetFile,
      replaces: file.replaces,
      verdict: file.verdict ? toVerdict(file.verdict) : null,
      refusal: file.refusal,
      refusalValues: file.refusalValues,
    })),
  };
}

export function toModsApplyOutcome(dto: BoundApplyOutcome): ModsApplyOutcome {
  switch (dto.kind) {
    case "accepted":
      return { kind: "accepted", operationId: dto.operationId, backupId: dto.backupId };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as ModsApplyRefusal };
  }
}

export function toOperationTailOutcome(dto: BoundTailOutcome): OperationTailOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: toRefusal(dto.refusal) };
  return { kind: "ready", tail: dto.tail };
}

export function toBackupModsOutcome(dto: BoundBackupModsOutcome): BackupModsOutcome {
  if (dto.kind === "refused") return { kind: "refused", refusal: toRefusal(dto.refusal) };
  const view = dto.view;
  return {
    kind: "ready",
    view: {
      state: toBackupModsState(view.state),
      recordedAt: view.recordedAt,
      game: view.game,
      counts: view.counts,
      mods: view.mods.map((item) => ({
        file: item.file,
        rank: item.rank,
        place: oneOf(BACKUP_PLACE, item.place),
        origin: oneOf(ORIGIN, item.origin),
        identity: item.identity ? toIdentity(item.identity) : null,
      })),
      compare: view.compare
        ? {
            complete: view.compare.complete,
            unread: view.compare.unread,
            same: view.compare.same,
            changes: view.compare.changes.map((change) => ({
              kind: oneOf(CHANGE_KIND, change.kind),
              id: change.id,
              name: change.name,
              file: change.file,
              backupVersion: change.backupVersion,
              currentVersion: change.currentVersion,
            })),
          }
        : null,
    },
  };
}
