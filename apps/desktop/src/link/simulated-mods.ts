import type { CancelOutcome } from "./deploy";
import type {
  BackupMods,
  BackupModsOutcome,
  ModCheckFile,
  ModIdentity,
  ModItem,
  ModsApplyOutcome,
  ModsApplyRefusal,
  ModsBatch,
  ModsCheckOutcome,
  ModsGame,
  ModsOutcome,
  ModsReadRefusal,
  ModsView,
  OperationTail,
  OperationTailOutcome,
} from "./mods";
import type { BackupRefusal } from "./serviceBackups";
import { toOperation } from "./serviceOps";
import type { SimulatedServiceOps } from "./simulated-service-ops";
import type { SimulatedServices } from "./simulated-services";
import { simSha256 } from "./simulated-transfers";

/**
 * Les mods d'un serveur Minecraft du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * juge une demande COMME LE VRAI (`crates/hearth-agent/src/application/mods.rs`) : service inconnu ou adopté, moteur sans
 * mods, lot hors bornes, fichier absent, mod du pack Modrinth, mod déjà actif, dépôt inconnu ou changé, opération déjà en
 * cours. La liste et les dépôts rendent ce que l'appelant a posé (`seed`, `registerDeposit`). La suite de l'opération
 * (étapes, issue, retour automatique) est PILOTÉE par l'appelant : `toStep`, `succeed`, `rollBack` ; rien n'avance seul.
 * Jamais livré (derrière `import.meta.env.DEV`).
 */

/** Les étapes du plan d'une opération de mods (`domain/ops/mods.rs`). */
export const SIM_MODS_STEPS = ["stop", "copy", "verify", "place", "start", "watch"] as const;
export type SimModsStep = (typeof SIM_MODS_STEPS)[number];

/** Un dépôt de la zone d'attente que le contrôle sait juger. */
export interface SimDeposit {
  identity: ModIdentity;
  sha256: string;
  /** Un refus que l'agent rendrait (`MOD_UNRECOGNIZED`, `MOD_TOO_LARGE`, etc.) ; absent : le dépôt se juge. */
  refusal?: string;
  /** `mismatch` : la version de Minecraft visée n'est pas celle du serveur. */
  minecraft?: "ok" | "mismatch";
  /** Les dépendances absentes (verdict « À compléter »). */
  missing?: string[];
}

interface Held {
  operationId: string;
  backupId: string;
  key: string;
  batch: ModsBatch;
  by: string;
}

const EMPTY_GAME: ModsGame = {
  loader: "fabric",
  minecraft: "1.20.1",
  minecraftFrom: "setting",
  updatesEveryStart: false,
  pack: "none",
};

export function simMod(patch: Partial<ModItem> = {}): ModItem {
  return {
    file: null,
    rank: 0,
    place: "active",
    enabled: !(patch.file ?? "").endsWith(".disabled"),
    origin: "manual",
    readableName: true,
    identity: null,
    verdict: { code: "compatible", missing: [], neededBy: [] },
    ...patch,
  };
}

export class SimulatedMods {
  private readonly views = new Map<string, ModsView>();
  private readonly deposits = new Map<string, SimDeposit>();
  private readonly expected = new Map<string, Omit<SimDeposit, "sha256"> & { sha256?: string }>();
  private readonly held = new Map<string, Map<string, Held>>();
  private readonly tails = new Map<string, OperationTail>();
  private readonly backupLists = new Map<string, BackupMods>();
  private nextId = 1;
  /** Les lectures de la liste, dans l'ordre. */
  readonly listed: { serverId: string; key: string }[] = [];
  /** Les contrôles reçus (dépôts). */
  readonly checked: { serverId: string; key: string; deposits: string[] }[] = [];
  /** Les lots reçus (sans mot de passe). */
  readonly received: { serverId: string; key: string; batch: ModsBatch }[] = [];
  /** Les lectures de la liste d'une sauvegarde. */
  readonly backupReads: { serverId: string; backupId: string; compare: boolean }[] = [];
  /** Les annulations reçues. */
  readonly cancelled: { serverId: string; opId: string }[] = [];
  /** `true` : la lecture de la liste échoue (lien sans réponse). */
  failList = false;
  /** Le refus de lecture que la prochaine lecture rendra (puis oublié). */
  readRefusal: ModsReadRefusal["kind"] | null = null;
  /** Jars « non lus » rendus par les prochaines lectures : décroît d'un cran à chaque lecture (gros serveur). */
  unreadSteps: number[] = [];

  constructor(
    private readonly services: SimulatedServices,
    private readonly ops: SimulatedServiceOps,
  ) {}

  private slot(serverId: string, key: string): string {
    return `${serverId}/${key}`;
  }

  /** Pose la liste que ce service rendra. */
  seed(
    serverId: string,
    key: string,
    view: Omit<Partial<ModsView>, "game"> & { game?: Partial<ModsGame> },
  ): void {
    this.views.set(this.slot(serverId, key), {
      applicable: view.applicable ?? "mods",
      game: { ...EMPTY_GAME, ...view.game },
      mods: (view.mods ?? []).map((mod, rank) => ({ ...mod, rank: mod.rank || rank })),
      unread: view.unread ?? 0,
      truncated: view.truncated ?? false,
    });
  }

  viewOf(serverId: string, key: string): ModsView | undefined {
    return this.views.get(this.slot(serverId, key));
  }

  /** Ce que l'agent d'essai lira dans le prochain fichier de ce nom envoyé (rattaché au dépôt que `startDeposit` ouvre). */
  willRead(fileName: string, deposit: Omit<SimDeposit, "sha256"> & { sha256?: string }): void {
    this.expected.set(fileName, deposit);
  }

  /** Un dépôt vient de s'ouvrir sous ce nom : l'agent d'essai le juge d'après ce qui a été annoncé. */
  attach(depositId: string, fileName: string): void {
    const expected = this.expected.get(fileName);
    if (!expected) return;
    this.deposits.set(depositId, { ...expected, sha256: expected.sha256 ?? simSha256(fileName) });
  }

  /** Pose un dépôt de la zone d'attente que le contrôle jugera. */
  registerDeposit(id: string, deposit: SimDeposit): void {
    this.deposits.set(id, deposit);
  }

  private known(serverId: string, key: string): ModsReadRefusal | null {
    const found = this.services.find(serverId, key);
    if (!found || found.origin !== "hearth") return { kind: "service_not_found" };
    if (found.adopted !== null) return { kind: "service_adopted" };
    return null;
  }

  list(serverId: string, key: string): ModsOutcome {
    this.listed.push({ serverId, key });
    if (this.readRefusal) {
      const kind = this.readRefusal;
      this.readRefusal = null;
      return { kind: "refused", refusal: { kind } as ModsReadRefusal };
    }
    const refusal = this.known(serverId, key);
    if (refusal) return { kind: "refused", refusal };
    const view = this.views.get(this.slot(serverId, key));
    if (!view) return { kind: "refused", refusal: { kind: "service_not_found" } };
    const unread = this.unreadSteps.length > 0 ? (this.unreadSteps.shift() ?? 0) : view.unread;
    return { kind: "ready", view: structuredClone({ ...view, unread }) };
  }

  check(serverId: string, key: string, deposits: string[]): ModsCheckOutcome {
    this.checked.push({ serverId, key, deposits: [...deposits] });
    const refusal = this.known(serverId, key);
    if (refusal) return { kind: "refused", refusal };
    const view = this.views.get(this.slot(serverId, key));
    if (!view || view.applicable !== "mods") {
      return { kind: "refused", refusal: { kind: "not_applicable" } };
    }
    if (deposits.some((id) => !this.deposits.has(id))) {
      return { kind: "refused", refusal: { kind: "deposit_unknown" } };
    }
    // Deux fichiers du même mod dans un lot : les deux sont refusés (BR-MOD-023).
    const counts = new Map<string, number>();
    for (const id of deposits) {
      const known = this.deposits.get(id)?.identity.id;
      if (known !== undefined) counts.set(known, (counts.get(known) ?? 0) + 1);
    }
    const files: ModCheckFile[] = deposits.map((id) => {
      const deposit = this.deposits.get(id) as SimDeposit;
      const refuse = (code: string, values?: ModCheckFile["refusalValues"]): ModCheckFile => ({
        deposit: id,
        outcome: "refused",
        identity: deposit.identity,
        targetFile: null,
        replaces: null,
        verdict: null,
        refusal: code,
        refusalValues: values ?? null,
      });
      if (deposit.refusal) return refuse(deposit.refusal);
      const loader = view.game.loader;
      const accepted =
        deposit.identity.loader === loader ||
        (loader === "quilt" && deposit.identity.loader === "fabric");
      if (!accepted) {
        return refuse("MOD_INCOMPATIBLE_LOADER", {
          loader: deposit.identity.loader,
          serverLoader: loader,
          serverMinecraft: view.game.minecraft,
        });
      }
      if (deposit.minecraft === "mismatch") {
        return refuse("MOD_INCOMPATIBLE_MINECRAFT", {
          loader: deposit.identity.loader,
          serverLoader: loader,
          serverMinecraft: view.game.minecraft,
        });
      }
      if (counts.get(deposit.identity.id) !== 1) return refuse("MOD_DUPLICATE_IN_BATCH");
      const target = `${deposit.identity.id}-${deposit.identity.version ?? "0"}.jar`;
      const installed = view.mods.find(
        (mod) => mod.place === "active" && mod.identity?.id === deposit.identity.id,
      );
      if (installed?.origin === "modrinth_pack") return refuse("MOD_IN_MODRINTH_PACK");
      if (installed && installed.identity?.version === deposit.identity.version) {
        return refuse("MOD_SAME_VERSION");
      }
      const missing = deposit.missing ?? [];
      const verdict = {
        code: missing.length > 0 ? ("to_complete" as const) : ("compatible" as const),
        missing,
        neededBy: [],
      };
      return {
        deposit: id,
        outcome: installed ? "replace" : "add",
        identity: deposit.identity,
        targetFile: target,
        replaces: installed?.file
          ? { file: installed.file, version: installed.identity?.version ?? null }
          : null,
        verdict,
        refusal: null,
        refusalValues: null,
      };
    });
    return { kind: "ready", files };
  }

  // ── Appliquer un lot ───────────────────────────────────────────────────────────────────────

  accept(serverId: string, key: string, batch: ModsBatch, by: string): ModsApplyOutcome {
    const mod = (
      code: string,
      extra: { deposit?: string; reason?: string } = {},
    ): ModsApplyOutcome => ({
      kind: "refused",
      refusal: {
        kind: "mod",
        code,
        deposit: extra.deposit ?? null,
        reason: extra.reason ?? null,
      } satisfies ModsApplyRefusal,
    });
    const common = (refusal: BackupRefusal): ModsApplyOutcome => ({
      kind: "refused",
      refusal: { kind: "common", refusal },
    });
    const found = this.services.find(serverId, key);
    if (!found || found.origin !== "hearth") return common({ kind: "service_not_found" });
    if (found.adopted !== null) return common({ kind: "service_adopted" });
    const view = this.views.get(this.slot(serverId, key));
    if (!view) return common({ kind: "service_not_found" });
    if (view.applicable !== "mods") return mod("MODS_NOT_APPLICABLE");
    if (found.busy) return mod("OPERATION_IN_PROGRESS");
    const total =
      batch.add.length +
      batch.retire.length +
      batch.restore.length +
      batch.disable.length +
      batch.enable.length;
    if (total < 1 || total > 64 || batch.add.length > 8) return mod("MODS_BATCH_INVALID");
    const active = (file: string) =>
      view.mods.find((item) => item.place === "active" && item.file === file);
    const retired = (file: string) =>
      view.mods.find((item) => item.place === "retired" && item.file === file);
    for (const add of batch.add) {
      const deposit = this.deposits.get(add.deposit);
      if (!deposit) return mod("DEPOSIT_UNKNOWN");
      if (deposit.sha256 !== add.sha256) {
        return mod("MOD_REFUSED", { deposit: add.deposit, reason: "MOD_DEPOSIT_CHANGED" });
      }
      if (add.replace !== null) {
        const old = active(add.replace);
        if (!old) return mod("MOD_FILE_MISSING");
        if (old.origin === "modrinth_pack") return mod("MOD_IN_MODRINTH_PACK");
        if (old.identity?.id !== deposit.identity.id) {
          return mod("MOD_REFUSED", { deposit: add.deposit, reason: "MOD_REPLACE_MISMATCH" });
        }
      }
    }
    for (const file of [...batch.retire, ...batch.disable]) {
      const item = active(file);
      if (!item) return mod("MOD_FILE_MISSING");
      if (item.origin === "modrinth_pack") return mod("MOD_IN_MODRINTH_PACK");
    }
    for (const file of batch.restore) if (!retired(file)) return mod("MOD_FILE_MISSING");
    for (const file of batch.enable) {
      const item = active(file);
      if (!item || item.enabled) return mod("MOD_FILE_MISSING");
      const same = view.mods.find(
        (other) =>
          other !== item &&
          other.place === "active" &&
          other.enabled &&
          other.identity !== null &&
          other.identity.id === item.identity?.id,
      );
      if (same) return mod("MOD_ALREADY_ACTIVE");
    }
    this.received.push({ serverId, key, batch: structuredClone(batch) });
    const operation = this.ops.accept(
      serverId,
      { key, kind: "mods", step: null, steps: [], cancellable: true },
      by,
    );
    const backupId = `01SIMMD${String(this.nextId++).padStart(6, "0")}`;
    const mine = this.held.get(serverId) ?? new Map<string, Held>();
    mine.set(operation.id, { operationId: operation.id, backupId, key, batch, by });
    this.held.set(serverId, mine);
    return { kind: "accepted", operationId: operation.id, backupId };
  }

  private heldOf(serverId: string, opId: string): Held {
    const found = this.held.get(serverId)?.get(opId);
    if (!found) throw new Error(`opération de mods inconnue : ${opId}`);
    return found;
  }

  private stepsUpTo(step: SimModsStep, last: "intent" | "done" = "intent") {
    const upto = SIM_MODS_STEPS.indexOf(step);
    return SIM_MODS_STEPS.slice(0, upto + 1).map((name, index) => ({
      step: name,
      state: index < upto ? ("done" as const) : last,
    }));
  }

  /** L'opération arrive à cette étape (l'annulation n'est plus possible à partir de `place`). */
  toStep(serverId: string, opId: string, step: SimModsStep): void {
    this.heldOf(serverId, opId);
    this.ops.update(serverId, opId, {
      status: "running",
      step,
      steps: this.stepsUpTo(step),
      cancellable: SIM_MODS_STEPS.indexOf(step) < SIM_MODS_STEPS.indexOf("place"),
    });
  }

  /** Le lot est en place et le serveur est reparti : la liste de ce service change comme l'agent la ferait. */
  succeed(serverId: string, opId: string): void {
    const held = this.heldOf(serverId, opId);
    const view = this.views.get(this.slot(serverId, held.key));
    if (view) this.place(view, held.batch);
    this.ops.update(serverId, opId, { steps: this.stepsUpTo("watch", "done") });
    this.ops.finish(serverId, opId, "succeeded", null);
    this.held.get(serverId)?.delete(opId);
  }

  /** Le jeu n'a pas dit « prêt » : l'agent revient seul à l'état d'avant (BR-MOD-027). */
  rollBack(serverId: string, opId: string, tail: OperationTail): void {
    this.heldOf(serverId, opId);
    this.tails.set(opId, tail);
    this.ops.finish(serverId, opId, "rolled_back", "stopped_right_after_start");
    this.held.get(serverId)?.delete(opId);
  }

  /** Une panne au milieu de la mise en place, mouvements défaits ou non. */
  fail(serverId: string, opId: string, status: "failed" | "interrupted" = "failed"): void {
    this.heldOf(serverId, opId);
    this.ops.finish(serverId, opId, status, "step_failed");
    this.held.get(serverId)?.delete(opId);
  }

  /** L'opération est une opération de mods en cours de ce serveur. */
  cancels(serverId: string, opId: string): boolean {
    return this.held.get(serverId)?.has(opId) ?? false;
  }

  /** `DELETE /operations/services/{id}` : possible jusqu'au début de la mise en place (BR-MOD-026). */
  cancel(serverId: string, opId: string): CancelOutcome {
    this.cancelled.push({ serverId, opId });
    const dto = this.ops.of(serverId).find((item) => item.id === opId);
    if (!dto) return { kind: "refused", refusal: { kind: "not_found" } };
    if (!dto.cancellable) return { kind: "refused", refusal: { kind: "not_cancellable" } };
    this.ops.finish(serverId, opId, "cancelled", "cancelled_by_account");
    this.held.get(serverId)?.delete(opId);
    const after = this.ops.of(serverId).find((item) => item.id === opId) ?? dto;
    return { kind: "done", operation: toOperation(after) };
  }

  private place(view: ModsView, batch: ModsBatch): void {
    const byFile = (file: string, place: ModItem["place"]) =>
      view.mods.find((item) => item.file === file && item.place === place);
    for (const add of batch.add) {
      const deposit = this.deposits.get(add.deposit);
      if (!deposit) continue;
      if (add.replace !== null) {
        const old = byFile(add.replace, "active");
        if (old) old.place = "retired";
      }
      view.mods.push(
        simMod({
          file: `${deposit.identity.id}-${deposit.identity.version ?? "0"}.jar`,
          origin: "hearth",
          identity: deposit.identity,
          rank: view.mods.length,
        }),
      );
    }
    for (const file of batch.retire) {
      const item = byFile(file, "active");
      if (item) item.place = "retired";
    }
    for (const file of batch.restore) {
      const item = byFile(file, "retired");
      if (item) item.place = "active";
    }
    for (const file of batch.disable) {
      const item = byFile(file, "active");
      if (item) {
        item.enabled = false;
        item.file = `${file}.disabled`;
      }
    }
    for (const file of batch.enable) {
      const item = byFile(file, "active");
      if (item) {
        item.enabled = true;
        item.file = file.replace(/\.disabled$/, "");
      }
    }
  }

  tail(opId: string): OperationTailOutcome {
    const tail = this.tails.get(opId);
    if (!tail) return { kind: "refused", refusal: { kind: "other" } };
    return { kind: "ready", tail };
  }

  // ── Les mods d'une sauvegarde (HRT-294) ──────────────────────────────────────────────────

  seedBackupMods(serverId: string, key: string, backupId: string, mods: BackupMods): void {
    this.backupLists.set(`${this.slot(serverId, key)}/${backupId}`, mods);
  }

  backupMods(serverId: string, key: string, backupId: string, compare: boolean): BackupModsOutcome {
    this.backupReads.push({ serverId, backupId, compare });
    const refusal = this.known(serverId, key);
    if (refusal) return { kind: "refused", refusal };
    const stored = this.backupLists.get(`${this.slot(serverId, key)}/${backupId}`);
    if (!stored) return { kind: "refused", refusal: { kind: "backup_not_found" } };
    const view: BackupMods = structuredClone(stored);
    if (!compare) view.compare = null;
    return { kind: "ready", view };
  }
}
