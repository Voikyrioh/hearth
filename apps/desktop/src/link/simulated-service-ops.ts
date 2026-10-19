import type { ServiceOperationEvent as BoundEvent, OperationDto } from "@/bindings";
import {
  type DecisionOutcome,
  isFinished,
  type OperationChoice,
  type ServiceOperationEvent,
  toDecisionOutcome,
  toOperation,
  toServiceOperationEvent,
} from "./serviceOps";
import { LinkCommandError, type Unsubscribe } from "./types";

/**
 * Les opérations de service du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent
 * que l'appelant PILOTE. Il produit les MÊMES formes que la coquille (événements du type généré
 * `ServiceOperationEvent` de `bindings.ts`) et tient les mêmes promesses :
 *
 * - une opération qui court encore au retour du lien est reconstruite SANS nouveau message ;
 * - l'issue d'une opération terminée pendant une coupure est annoncée UNE fois au retour du lien ;
 * - une issue reste retenue jusqu'à son acquittement (`ack`), rejouée à l'abonnement ;
 * - un compte ne voit que les opérations de services qu'il peut voir (BR-OPS-001) : tout service créé par
 *   Hearth (`h_…`), les autres seulement avec le droit « gérer les services ».
 *
 * Aucune opération réelle n'existe encore : le simulé en fournit trois de démonstration (`seedDemo`). Jamais
 * livré (derrière `import.meta.env.DEV`).
 */

interface Held {
  seq: number;
  dto: OperationDto;
}

/** Ce qu'un test peut préciser d'une opération ; le reste a des valeurs de départ. */
export interface SimOperationPatch {
  id?: string;
  key: string;
  kind?: OperationDto["kind"];
  status?: OperationDto["status"];
  step?: string | null;
  steps?: OperationDto["steps"];
  percent?: number | null;
  itemsDone?: number | null;
  bytesDone?: number | null;
  bytesTotal?: number | null;
  etaS?: number | null;
  cancellable?: boolean;
  cause?: OperationDto["cause"];
}

export class SimulatedServiceOps {
  private readonly held = new Map<string, Map<string, Held>>();
  /** Les changements faits hors lien, annoncés une fois le lien revenu (comme la relecture de la liaison). */
  private readonly missed = new Map<string, Set<string>>();
  private readonly listeners = new Set<(event: ServiceOperationEvent) => void>();
  private seq = 0;
  private nextId = 1;
  /** Chaque décision reçue, dans l'ordre (de quoi vérifier ce qui part, sans mot de passe). */
  readonly decisions: { serverId: string; opId: string; choice: OperationChoice }[] = [];
  /** Les issues acquittées par l'interface, dans l'ordre. */
  readonly acknowledged: { serverId: string; ids: string[] }[] = [];
  /** `true` : l'abonnement échoue. */
  failSubscribe = false;
  subscriptions = 0;

  constructor(
    private readonly connected: (serverId: string) => boolean,
    /** Ce compte peut-il voir les opérations de ce service ? (BR-OPS-001) */
    private readonly visible: (serverId: string, key: string) => boolean,
    /** Pose ou retire l'action qui tient le service (`ServiceItem.busy`, BR-OPS-005). */
    private readonly setBusy: (
      serverId: string,
      key: string,
      busy: { kind: string; by: string; since: string } | null,
    ) => void,
    private readonly now: () => number,
  ) {}

  // ── Pilotage ─────────────────────────────────────────────────────────────────────────────────

  /** Accepte une opération (en cours par défaut) lancée par le compte `by`. */
  accept(serverId: string, patch: SimOperationPatch, by = "marie"): OperationDto {
    const id = patch.id ?? `01SIMOP${String(this.nextId++).padStart(6, "0")}`;
    const dto: OperationDto = {
      id,
      key: patch.key,
      kind: patch.kind ?? "deploy",
      status: patch.status ?? "running",
      step: patch.step ?? null,
      steps: patch.steps ?? [],
      percent: patch.percent ?? null,
      itemsDone: patch.itemsDone ?? null,
      bytesDone: patch.bytesDone ?? null,
      bytesTotal: patch.bytesTotal ?? null,
      etaS: patch.etaS ?? null,
      cancellable: patch.cancellable ?? true,
      decision: null,
      cause: patch.cause ?? null,
    };
    this.put(serverId, dto);
    if (!isFinished(dto.status) && this.visible(serverId, dto.key)) {
      this.setBusy(serverId, dto.key, {
        kind: dto.kind,
        by,
        since: new Date(this.now()).toISOString(),
      });
    }
    return dto;
  }

  /** Change une opération en cours (étape, progression, statut). */
  update(serverId: string, id: string, patch: Partial<Omit<OperationDto, "id" | "key">>): void {
    const held = this.held.get(serverId)?.get(id);
    if (!held) return;
    this.put(serverId, { ...held.dto, ...patch });
  }

  /** L'opération attend une décision du compte (`BR-JEU-006`, `BR-EDIT-031`) ; `expiresAt` null : sans limite. */
  askDecision(
    serverId: string,
    id: string,
    choices: OperationChoice[],
    expiresAt: number | null = this.now() + 15 * 60_000,
    status: "awaiting_decision" | "needs_attention" = "awaiting_decision",
  ): void {
    this.update(serverId, id, { status, decision: { choices, expiresAt } });
  }

  /** L'opération se termine : le service n'est plus tenu, l'issue est annoncée une fois. */
  finish(
    serverId: string,
    id: string,
    status: Extract<
      OperationDto["status"],
      "cancelled" | "rolled_back" | "succeeded" | "failed" | "interrupted"
    > = "succeeded",
    cause: OperationDto["cause"] = null,
  ): void {
    const held = this.held.get(serverId)?.get(id);
    if (!held) return;
    this.setBusy(serverId, held.dto.key, null);
    this.put(serverId, { ...held.dto, status, decision: null, cause, cancellable: false });
  }

  /** L'agent ne connaît plus cette opération (purgée, ou service que le compte ne voit plus). */
  gone(serverId: string, id: string): void {
    const bucket = this.held.get(serverId);
    const held = bucket?.get(id);
    if (!bucket || !held) return;
    bucket.delete(id);
    this.setBusy(serverId, held.dto.key, null);
    this.seq += 1;
    this.deliver({ kind: "gone", serverId, seq: this.seq, id });
  }

  /** Le lien revient : ce qui a changé hors lien est annoncé UNE fois ; le reste est reconstruit sans message. */
  resync(serverId: string): void {
    const ids = this.missed.get(serverId);
    this.missed.delete(serverId);
    if (!ids) return;
    for (const id of ids) {
      const held = this.held.get(serverId)?.get(id);
      if (held) this.deliver(this.upsertEvent(serverId, held));
    }
  }

  /** Les opérations qu'un serveur tient (de quoi vérifier). */
  of(serverId: string): OperationDto[] {
    return [...(this.held.get(serverId)?.values() ?? [])].map((held) => held.dto);
  }

  /** Trois opérations de démonstration (aucune opération réelle n'existe encore) sur des services de `forge`. */
  seedDemo(serverId: string, keys: { deploy: string; update: string; wait: string }): void {
    this.accept(
      serverId,
      {
        id: "01SIMDEMO000000000000DEPL",
        key: keys.deploy,
        kind: "deploy",
        step: "pull",
        percent: 40,
        bytesDone: 400_000_000,
        bytesTotal: 1_000_000_000,
      },
      "marie",
    );
    this.accept(
      serverId,
      { id: "01SIMDEMO000000000000UPDT", key: keys.update, kind: "update", step: "recreate" },
      "claire",
    );
    const waiting = this.accept(
      serverId,
      { id: "01SIMDEMO000000000000WAIT", key: keys.wait, kind: "update", step: "verify" },
      "marie",
    );
    this.askDecision(serverId, waiting.id, ["revert", "keep"]);
  }

  // ── Ce que voit le pont ───────────────────────────────────────────────────────────────────────

  async subscribe(listener: (event: ServiceOperationEvent) => void): Promise<Unsubscribe> {
    this.subscriptions += 1;
    if (this.failSubscribe) throw new LinkCommandError({ kind: "internal" });
    this.listeners.add(listener);
    for (const [serverId, bucket] of this.held) {
      for (const held of bucket.values()) {
        if (this.visible(serverId, held.dto.key))
          listener(this.convert(this.upsertEvent(serverId, held)));
      }
    }
    return () => void this.listeners.delete(listener);
  }

  async ack(serverId: string, ids: string[]): Promise<void> {
    this.acknowledged.push({ serverId, ids: [...ids] });
    const bucket = this.held.get(serverId);
    if (!bucket) return;
    for (const id of ids) {
      const held = bucket.get(id);
      if (held && isFinished(held.dto.status)) bucket.delete(id);
    }
  }

  /** Une décision (l'appelant a déjà vérifié le droit et la confirmation). */
  decide(serverId: string, opId: string, choice: OperationChoice): DecisionOutcome {
    this.decisions.push({ serverId, opId, choice });
    const held = this.held.get(serverId)?.get(opId);
    if (!held || !this.visible(serverId, held.dto.key)) {
      return toDecisionOutcome({ kind: "refused", refusal: { kind: "not_found" } });
    }
    const decision = held.dto.decision;
    if (!decision || isFinished(held.dto.status)) {
      return toDecisionOutcome({ kind: "refused", refusal: { kind: "not_waiting" } });
    }
    if (decision.expiresAt !== null && decision.expiresAt <= this.now()) {
      return toDecisionOutcome({ kind: "refused", refusal: { kind: "expired" } });
    }
    if (!decision.choices.includes(choice)) {
      return toDecisionOutcome({ kind: "refused", refusal: { kind: "not_offered" } });
    }
    // « Revenir » démarre un retour arrière, jamais un « revenu » sans geste (BR-EDIT-031) ; les autres choix
    // laissent l'opération continuer ou la ferment sans rien changer.
    const next: OperationDto =
      choice === "revert"
        ? { ...held.dto, status: "rolling_back", decision: null, cause: "decided_by_account" }
        : choice === "cancel"
          ? { ...held.dto, status: "cancelling", decision: null, cause: "cancelled_by_account" }
          : choice === "continue"
            ? { ...held.dto, status: "running", decision: null, cause: "decided_by_account" }
            : { ...held.dto, status: "succeeded", decision: null, cause: "decided_by_account" };
    if (isFinished(next.status)) this.setBusy(serverId, next.key, null);
    this.put(serverId, next);
    return { kind: "done", operation: toOperation(next) };
  }

  // ── Interne ───────────────────────────────────────────────────────────────────────────────────

  private put(serverId: string, dto: OperationDto): void {
    this.seq += 1;
    let bucket = this.held.get(serverId);
    if (!bucket) {
      bucket = new Map();
      this.held.set(serverId, bucket);
    }
    const held: Held = { seq: this.seq, dto };
    bucket.set(dto.id, held);
    if (!this.visible(serverId, dto.key)) return;
    if (!this.connected(serverId)) {
      let ids = this.missed.get(serverId);
      if (!ids) {
        ids = new Set();
        this.missed.set(serverId, ids);
      }
      ids.add(dto.id);
      return;
    }
    this.deliver(this.upsertEvent(serverId, held));
  }

  private upsertEvent(serverId: string, held: Held): BoundEvent {
    return {
      serverId,
      seq: held.seq,
      change: { kind: "upsert", operation: held.dto },
    };
  }

  private convert(event: BoundEvent): ServiceOperationEvent {
    return toServiceOperationEvent(event);
  }

  private deliver(event: BoundEvent | ServiceOperationEvent): void {
    const converted = "change" in event ? this.convert(event) : event;
    for (const listener of [...this.listeners]) listener(converted);
  }
}
