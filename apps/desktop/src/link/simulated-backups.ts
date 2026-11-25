import type { ServiceDto } from "@/bindings";
import type { CancelOutcome } from "./deploy";
import type {
  BackupCreateOutcome,
  BackupDeleteOutcome,
  BackupInfo,
  BackupMode,
  BackupPreview,
  BackupPreviewOutcome,
  BackupReadRefusal,
  BackupRefusal,
  BackupRequest,
  BackupSettings,
  BackupSettingsOutcome,
  BackupsOutcome,
  BackupVerifyOutcome,
  IgnoredView,
} from "./serviceBackups";
import { toOperation } from "./serviceOps";
import type { SimulatedServiceOps } from "./simulated-service-ops";
import type { SimulatedServices } from "./simulated-services";

/**
 * Les sauvegardes d'un service du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui juge
 * une demande COMME LE VRAI (`crates/hearth-agent/src/application/backups.rs`) : service inconnu ou adopté, action en
 * cours, place déjà prise par une autre sauvegarde sur le serveur, état qui ne se sauvegarde pas, mode et note, rien à
 * sauvegarder, espace sous la réserve (aucune case pour passer outre). La liste et l'estimation rendent ce que l'appelant
 * a posé (`seed`, `setPreview`). La suite de l'opération (étapes, progression, issue) est PILOTÉE par l'appelant :
 * `toStep`, `progress`, `succeed`, `fail` ; rien n'avance seul. Jamais livré (derrière `import.meta.env.DEV`).
 */

const GIB = 1024 ** 3;

/** Les états où une sauvegarde se crée (miroir de ce que l'agent accepte, BR-BACKUP-006). */
const SAVABLE: readonly ServiceDto["state"][] = ["running", "stopped", "error", "restart_loop"];

/** Les étapes du plan d'une sauvegarde (`domain/ops/backup.rs`) : l'arrêt et la relance seulement avec l'arrêt voulu. */
export const SIM_BACKUP_STEPS = ["stop", "copy", "verify", "start"] as const;
export type SimBackupStep = (typeof SIM_BACKUP_STEPS)[number];

interface Held {
  operationId: string;
  backupId: string;
  key: string;
  name: string;
  request: BackupRequest;
  running: boolean;
  by: string;
}

function emptyBackup(id: string, patch: Partial<BackupInfo>): BackupInfo {
  return {
    id,
    createdAt: "2026-11-13T21:05:00Z",
    archiveBytes: 3.4 * GIB,
    dataBytes: 5.1 * GIB,
    files: 48_210,
    mode: "hot",
    flags: ["hot"],
    changedFiles: 0,
    skipped: 0,
    note: null,
    account: "marie",
    image: "ghcr.io/example/terraria:1.4.4",
    integrity: "verified",
    verifiedAt: "2026-11-13T21:07:00Z",
    mods: null,
    ...patch,
  };
}

export class SimulatedBackups {
  private readonly lists = new Map<string, BackupInfo[]>();
  private readonly settings = new Map<string, BackupSettings>();
  private readonly previews = new Map<string, Partial<BackupPreview>>();
  private readonly frees = new Map<string, number | null>();
  private readonly sameDisks = new Set<string>();
  private readonly held = new Map<string, Map<string, Held>>();
  private nextId = 1;
  /** Les listes lues, dans l'ordre. */
  readonly listed: { serverId: string; key: string }[] = [];
  /** Les estimations lues, dans l'ordre. */
  readonly previewed: { serverId: string; key: string }[] = [];
  /** Les demandes de création reçues (sans mot de passe) ; la note y est, pour vérifier ce qui part. */
  readonly received: { serverId: string; key: string; request: BackupRequest }[] = [];
  /** Les vérifications, suppressions et enregistrements reçus. */
  readonly verified: { serverId: string; backupId: string }[] = [];
  readonly deleted: { serverId: string; backupId: string }[] = [];
  readonly saved: { serverId: string; key: string; settings: BackupSettings }[] = [];
  readonly cancels: { serverId: string; opId: string }[] = [];
  /** `true` : la lecture de la liste échoue (lien sans réponse), pour un test. */
  failList = false;
  /** `true` : l'estimation échoue. */
  failPreview = false;
  /** Retard de l'estimation (ms) : la mesure des données dure (BR-BACKUP-009). */
  previewDelayMs = 0;
  /** Retard de la vérification (ms). */
  verifyDelayMs = 0;
  /** Le résultat que la prochaine vérification rendra. */
  verifyResult: "verified" | "corrupt" | "missing" = "verified";
  /** `true` : la suppression est refusée, la sauvegarde servant à une opération (BR-BACKUP-027). */
  deleteInUse = false;
  /** Le refus de lecture que la prochaine lecture rendra (puis oublié), pour un test. */
  readRefusal: "path_refused" | null = null;

  constructor(
    private readonly services: SimulatedServices,
    private readonly ops: SimulatedServiceOps,
  ) {}

  private slot(serverId: string, key: string): string {
    return `${serverId}/${key}`;
  }

  /** Pose les sauvegardes que la liste de ce service rendra (les plus récentes d'abord). */
  seed(serverId: string, key: string, backups: Partial<BackupInfo>[]): void {
    this.lists.set(
      this.slot(serverId, key),
      backups.map((patch) =>
        emptyBackup(patch.id ?? `01SIMBK${String(this.nextId++).padStart(6, "0")}`, patch),
      ),
    );
  }

  /** La sauvegarde que l'agent fait avant un lot de mods : elle rejoint la liste du service, sous le nom que l'opération rend. */
  /**
   * La sauvegarde « Avant mods » : faite APRÈS l'arrêt du serveur (mode « Arrêter, sauvegarder, relancer », jamais « à
   * chaud ») et avec la liste des mods d'avant (BR-MOD-034, 035).
   */
  addModsBackup(
    serverId: string,
    key: string,
    id: string,
    note: string,
    mods: BackupInfo["mods"] = null,
  ): void {
    const created = new Date().toISOString();
    const slot = this.slot(serverId, key);
    const entry = emptyBackup(id, {
      createdAt: created,
      note,
      mode: "stop_copy_start",
      flags: [],
      mods,
      integrity: "verified",
      verifiedAt: created,
    });
    this.lists.set(slot, [entry, ...(this.lists.get(slot) ?? [])]);
  }

  setPreview(serverId: string, key: string, preview: Partial<BackupPreview>): void {
    this.previews.set(this.slot(serverId, key), preview);
  }

  /** L'espace libre du disque des sauvegardes de ce serveur (`null` : la mesure a échoué). */
  setFree(serverId: string, freeBytes: number | null): void {
    this.frees.set(serverId, freeBytes);
  }

  setSameDisk(serverId: string, same: boolean): void {
    if (same) this.sameDisks.add(serverId);
    else this.sameDisks.delete(serverId);
  }

  backupsOf(serverId: string, key: string): BackupInfo[] {
    return this.lists.get(this.slot(serverId, key)) ?? [];
  }

  private free(serverId: string): number | null {
    return this.frees.has(serverId) ? (this.frees.get(serverId) ?? null) : 340 * GIB;
  }

  private judge(
    serverId: string,
    key: string,
  ): { refusal: BackupReadRefusal } | { service: ServiceDto } {
    const engine = this.services.engineState(serverId);
    // Comme l'agent : le moteur d'abord, puis la racine, puis le service.
    if (engine !== "ready") {
      return { refusal: { kind: "engine_unavailable", engine } };
    }
    if (this.readRefusal) {
      const reason = this.readRefusal;
      this.readRefusal = null;
      return { refusal: { kind: reason, reason: "backups_root_overlap" } };
    }
    const found = this.services.find(serverId, key);
    if (!found || (found.origin !== "hearth" && found.adopted === null)) {
      return { refusal: { kind: "service_not_found" } };
    }
    if (found.adopted !== null) return { refusal: { kind: "service_adopted" } };
    return { service: found };
  }

  /** `GET /services/{key}/backups`. */
  list(serverId: string, key: string): BackupsOutcome {
    this.listed.push({ serverId, key });
    const judged = this.judge(serverId, key);
    if ("refusal" in judged) return { kind: "refused", refusal: judged.refusal };
    const backups = [...this.backupsOf(serverId, key)];
    const total = backups.reduce((sum, backup) => sum + backup.archiveBytes, 0);
    const hasFolders = judged.service.mounts.some((mount) => mount.kind === "folder");
    return {
      kind: "ready",
      view: {
        backups,
        totalBytes: total,
        freeBytes: this.free(serverId),
        sameDisk: this.sameDisks.has(serverId),
        settings: structuredClone(
          this.settings.get(this.slot(serverId, key)) ?? { ignored: [], included: [] },
        ),
        nothingToSave: !hasFolders,
      },
    };
  }

  private defaultIgnored(service: ServiceDto): IgnoredView[] {
    const root = service.mounts.find((mount) => mount.kind === "folder")?.target;
    return root ? [{ root, path: "backups", why: "looks_like_backups" }] : [];
  }

  /** `POST …/backups/preview`. */
  preview(serverId: string, key: string): BackupPreviewOutcome {
    this.previewed.push({ serverId, key });
    const judged = this.judge(serverId, key);
    if ("refusal" in judged) return { kind: "refused", refusal: judged.refusal };
    const service = judged.service;
    const roots = service.mounts.filter((mount) => mount.kind === "folder").map((m) => m.target);
    const posed = this.previews.get(this.slot(serverId, key)) ?? {};
    const free = this.free(serverId);
    const bytes = posed.bytes ?? 5.1 * GIB;
    const reserve = Math.max(5 * GIB, (free ?? 0) * 0.05);
    const chosen: IgnoredView[] = (this.settings.get(this.slot(serverId, key))?.ignored ?? []).map(
      (item) => ({ ...item, why: "chosen" as const }),
    );
    return {
      kind: "ready",
      preview: {
        name: service.name,
        running: service.state === "running" || service.state === "restart_loop",
        stopGraceS: service.graceS ?? 10,
        roots,
        notSaved: [],
        bytes,
        files: 48_210,
        partial: false,
        bigFiles: [],
        ignored: [...this.defaultIgnored(service), ...chosen],
        ignoredMore: 0,
        freeBytes: free,
        reserveBytes: reserve,
        enoughSpace: free === null ? true : bytes + reserve <= free,
        sameDisk: this.sameDisks.has(serverId),
        nothingToSave: roots.length === 0,
        ...posed,
      },
    };
  }

  /**
   * Juge et accepte une demande, ou la refuse. Les contrôles dans l'ordre de l'agent : service, action en cours, place
   * du serveur, état, mode, note, éléments, rien à sauvegarder, espace. Le droit, la confirmation et le moteur ont déjà
   * été jugés par l'appelant.
   */
  accept(serverId: string, key: string, request: BackupRequest, by: string): BackupCreateOutcome {
    const refuse = (refusal: BackupRefusal): BackupCreateOutcome => ({ kind: "refused", refusal });
    const found = this.services.find(serverId, key);
    if (!found || (found.origin !== "hearth" && found.adopted === null)) {
      return refuse({ kind: "service_not_found" });
    }
    if (found.adopted !== null) return refuse({ kind: "service_adopted" });
    if (found.busy) {
      return refuse({
        kind: "service_busy",
        by: found.busy.by,
        action: found.busy.kind as never,
      });
    }
    if ([...(this.held.get(serverId)?.values() ?? [])].length > 0) {
      return refuse({ kind: "heavy_io_busy" });
    }
    if (!SAVABLE.includes(found.state)) return refuse({ kind: "not_drivable", state: found.state });
    const running = found.state === "running" || found.state === "restart_loop";
    if (request.mode === "stop_copy_start" && !running) return refuse({ kind: "mode_refused" });
    if (request.mode === "unknown") return refuse({ kind: "mode_refused" });
    if (request.note !== null && [...request.note].length > 80) {
      return refuse({ kind: "invalid_input", field: "note" });
    }
    if (request.ignoreOnce.length > 500) return refuse({ kind: "invalid_input", field: "items" });
    if (!found.mounts.some((mount) => mount.kind === "folder")) {
      return refuse({ kind: "nothing_to_save" });
    }
    const posed = this.previews.get(this.slot(serverId, key)) ?? {};
    const free = this.free(serverId);
    const bytes = posed.bytes ?? 5.1 * GIB;
    const reserve = posed.reserveBytes ?? Math.max(5 * GIB, (free ?? 0) * 0.05);
    if (posed.enoughSpace === false || (free !== null && bytes + reserve > free)) {
      return refuse({
        kind: "no_space",
        needed_bytes: bytes,
        free_bytes: free,
        reserve_bytes: reserve,
      });
    }
    this.received.push({ serverId, key, request: structuredClone(request) });
    const operation = this.ops.accept(
      serverId,
      { key, kind: "backup", step: null, steps: [], cancellable: true },
      by,
    );
    const backupId = `01SIMBK${String(this.nextId++).padStart(6, "0")}`;
    const mine = this.held.get(serverId) ?? new Map<string, Held>();
    mine.set(operation.id, {
      operationId: operation.id,
      backupId,
      key,
      name: found.name,
      request,
      running,
      by,
    });
    this.held.set(serverId, mine);
    return { kind: "accepted", operationId: operation.id, backupId };
  }

  // ── La suite de l'opération, pilotée par l'appelant ──────────────────────────────────────────

  private heldOf(serverId: string, opId: string): Held {
    const found = this.held.get(serverId)?.get(opId);
    if (!found) throw new Error(`sauvegarde inconnue : ${opId}`);
    return found;
  }

  private planOf(held: Held): SimBackupStep[] {
    const stop = held.request.mode === "stop_copy_start" && held.running;
    return SIM_BACKUP_STEPS.filter((name) => (name === "stop" || name === "start" ? stop : true));
  }

  private stepsUpTo(held: Held, step: SimBackupStep, last: "intent" | "done" = "intent") {
    const wanted = this.planOf(held);
    const index = wanted.indexOf(step);
    return wanted.slice(0, index + 1).map((name, i) => ({
      step: name as string,
      state: i < index ? ("done" as const) : last,
    }));
  }

  /** Passe à une étape ; l'annulation reste possible jusqu'à la fin. */
  toStep(serverId: string, opId: string, step: SimBackupStep): void {
    const held = this.heldOf(serverId, opId);
    this.ops.update(serverId, opId, {
      status: "running",
      step,
      steps: this.stepsUpTo(held, step),
      cancellable: true,
    });
  }

  /** La copie progresse (BR-BACKUP-011). */
  progress(
    serverId: string,
    opId: string,
    figures: { itemsDone: number; bytesDone: number; bytesTotal: number },
  ): void {
    const held = this.heldOf(serverId, opId);
    this.ops.update(serverId, opId, {
      status: "running",
      step: "copy",
      steps: this.stepsUpTo(held, "copy"),
      percent: Math.round((figures.bytesDone / figures.bytesTotal) * 100),
      itemsDone: figures.itemsDone,
      bytesDone: figures.bytesDone,
      bytesTotal: figures.bytesTotal,
      cancellable: true,
    });
  }

  /**
   * La sauvegarde réussit : elle entre dans la liste (la plus récente en tête), vérifiée. `cause: "not_restarted"` : le
   * service arrêté pour la copie ne repart pas (BR-BACKUP-008). `changedFiles` : fichiers changés pendant une copie à
   * chaud (BR-BACKUP-007).
   */
  succeed(
    serverId: string,
    opId: string,
    options: {
      cause?: "not_restarted" | null;
      changedFiles?: number;
      skipped?: number;
      createdAt?: string;
    } = {},
  ): void {
    const held = this.heldOf(serverId, opId);
    const mode: BackupMode = held.running ? held.request.mode : "stopped";
    const changed = options.changedFiles ?? 0;
    const flags: BackupInfo["flags"] = [];
    if (mode === "hot") flags.push("hot");
    if (mode === "hot" && changed > 0) flags.push("maybe_inconsistent");
    const created = options.createdAt ?? new Date().toISOString();
    const entry = emptyBackup(held.backupId, {
      createdAt: created,
      mode,
      flags,
      changedFiles: changed,
      skipped: options.skipped ?? 0,
      note: held.request.note,
      account: held.by,
      verifiedAt: created,
      integrity: "verified",
    });
    const slot = this.slot(serverId, held.key);
    this.lists.set(slot, [entry, ...(this.lists.get(slot) ?? [])]);
    this.ops.update(serverId, opId, {
      steps: this.stepsUpTo(held, this.planOf(held).at(-1) ?? "verify", "done"),
    });
    this.ops.finish(serverId, opId, "succeeded", options.cause ?? null);
    this.release(serverId, opId);
  }

  /** La sauvegarde échoue : rien n'est conservé, la liste ne change pas. */
  fail(
    serverId: string,
    opId: string,
    cause: Parameters<SimulatedServiceOps["finish"]>[3] = "backup_corrupt",
    status: "failed" | "interrupted" = "failed",
  ): void {
    this.heldOf(serverId, opId);
    this.ops.finish(serverId, opId, status, cause);
    this.release(serverId, opId);
  }

  private release(serverId: string, opId: string): void {
    this.held.get(serverId)?.delete(opId);
  }

  /** `DELETE /operations/services/{id}` : annule la sauvegarde ; rien n'est conservé. */
  cancel(serverId: string, opId: string): CancelOutcome {
    this.cancels.push({ serverId, opId });
    const dto = this.ops.of(serverId).find((item) => item.id === opId);
    if (!dto) return { kind: "refused", refusal: { kind: "not_found" } };
    if (["cancelled", "succeeded", "failed", "interrupted"].includes(dto.status)) {
      return { kind: "refused", refusal: { kind: "not_cancellable" } };
    }
    this.ops.update(serverId, opId, {
      status: "cancelling",
      cancellable: false,
      cause: "cancelled_by_account",
    });
    setTimeout(() => {
      this.ops.finish(serverId, opId, "cancelled", "cancelled_by_account");
      this.release(serverId, opId);
    }, 0);
    const after = this.ops.of(serverId).find((item) => item.id === opId) ?? dto;
    return { kind: "done", operation: toOperation(after) };
  }

  /** Les sauvegardes en cours d'un serveur (de quoi vérifier). */
  running(
    serverId: string,
  ): { operationId: string; backupId: string; key: string; name: string }[] {
    return [...(this.held.get(serverId)?.values() ?? [])].map((held) => ({
      operationId: held.operationId,
      backupId: held.backupId,
      key: held.key,
      name: held.name,
    }));
  }

  // ── Vérifier, supprimer, régler ──────────────────────────────────────────────────────────────

  private find(serverId: string, backupId: string): { slot: string; backup: BackupInfo } | null {
    for (const [slot, list] of this.lists) {
      if (!slot.startsWith(`${serverId}/`)) continue;
      const backup = list.find((item) => item.id === backupId);
      if (backup) return { slot, backup };
    }
    return null;
  }

  /** `POST /backups/{id}/verify`. */
  verify(serverId: string, backupId: string): BackupVerifyOutcome {
    this.verified.push({ serverId, backupId });
    const found = this.find(serverId, backupId);
    if (!found) return { kind: "refused", refusal: { kind: "backup_not_found" } };
    if (this.deleteInUse) return { kind: "refused", refusal: { kind: "backup_in_use" } };
    found.backup.integrity = this.verifyResult;
    found.backup.verifiedAt = this.verifyResult === "verified" ? new Date().toISOString() : null;
    return {
      kind: "done",
      result: {
        integrity: found.backup.integrity,
        verifiedAt: found.backup.verifiedAt,
        filesChecked: found.backup.files,
      },
    };
  }

  /** `DELETE /backups/{id}` : refusée tant qu'une opération utilise la sauvegarde. */
  remove(serverId: string, backupId: string): BackupDeleteOutcome {
    this.deleted.push({ serverId, backupId });
    const found = this.find(serverId, backupId);
    if (!found) {
      return { kind: "refused", refusal: { kind: "backup_not_found" } };
    }
    if (this.deleteInUse) return { kind: "refused", refusal: { kind: "backup_in_use" } };
    this.lists.set(
      found.slot,
      (this.lists.get(found.slot) ?? []).filter((item) => item.id !== backupId),
    );
    return { kind: "done", removedFiles: found.backup.integrity !== "missing" };
  }

  /** `PUT …/backup-settings`. */
  saveSettings(serverId: string, key: string, settings: BackupSettings): BackupSettingsOutcome {
    this.saved.push({ serverId, key, settings: structuredClone(settings) });
    const found = this.services.find(serverId, key);
    if (!found || (found.origin !== "hearth" && found.adopted === null)) {
      return { kind: "refused", refusal: { kind: "service_not_found" } };
    }
    if (found.adopted !== null) return { kind: "refused", refusal: { kind: "service_adopted" } };
    if (settings.ignored.length + settings.included.length > 500) {
      return { kind: "refused", refusal: { kind: "invalid_input", field: "items" } };
    }
    this.settings.set(this.slot(serverId, key), structuredClone(settings));
    return { kind: "done", settings: structuredClone(settings) };
  }
}
