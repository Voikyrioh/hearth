import type { ServiceDto } from "@/bindings";
import {
  commandIssue,
  envNameIssue,
  envValueIssue,
  imageIssue,
  MAX_ENV_VARS,
  MAX_PORT_MAPPINGS,
  nameIssue,
  splitImage,
  tagIssue,
} from "@/deploy/rules";
import { type EngineCapacity, engineCapacityFromWire, GIB } from "@/resources";
import {
  type CancelOutcome,
  DEPLOY_STEPS,
  type DeployCheck,
  type DeployCheckInput,
  type DeployField,
  type DeployOutcome,
  type DeployPort,
  type DeployProtocol,
  type DeployRefusal,
  type DeployRequest,
  type DeployStep,
  type NameStatus,
  type PortCheck,
  type PortStatus,
} from "./deploy";
import { toOperation } from "./serviceOps";
import type { SimulatedServiceOps } from "./simulated-service-ops";
import type { SimulatedServices } from "./simulated-services";

/**
 * Le déploiement d'une image du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * juge une demande COMME LE VRAI (`crates/hearth-agent/src/application/deployments.rs`, `domain/services/{deploy,ports}.rs`) :
 * même ordre de refus (moteur, validation, deux déploiements au plus, nom, ports, place sur le disque), mêmes constats
 * de ports (`verdict`, `suggest`), mêmes bornes de ressources. Un test qui passe ici passerait contre l'agent ; le
 * simulateur n'est pas plus indulgent que lui. La suite de l'opération (étapes, progression, issue) est PILOTÉE par
 * l'appelant : `progress`, `toStep`, `succeed`, `fail` ; rien n'avance seul.
 *
 * Les règles de FORME sont celles de `deploy/rules.ts` (le miroir du domaine de l'agent, testé contre ses vecteurs) ;
 * les règles de CONFLIT (qui tient quoi) sont ici, parce que le client n'en connaît aucune. Jamais livré (derrière
 * `import.meta.env.DEV`).
 */

const LOW_DISK_BYTES = 10 * GIB;
const MIN_CPU_MILLIS = 250;
const MIN_MEMORY_BYTES = 128 * 1024 * 1024;

interface Listener {
  program: string | null;
}

interface Held {
  operationId: string;
  key: string;
  name: string;
  step: DeployStep;
  image: string;
  by: string;
}

/** Une demande reçue, telle que le serveur la garde pour un test : jamais une valeur dans un état de l'interface. */
export interface ReceivedDeploy {
  serverId: string;
  request: DeployRequest;
}

export class SimulatedDeploys {
  /** Les demandes ACCEPTÉES, dans l'ordre (de quoi vérifier ce qui part ; le vrai agent ne garde aucun secret en clair). */
  readonly received: ReceivedDeploy[] = [];
  /** Chaque consultation reçue, dans l'ordre : elle ne porte jamais un secret. */
  readonly checks: { serverId: string; input: DeployCheckInput }[] = [];
  /** Chaque annulation reçue. */
  readonly cancels: { serverId: string; opId: string }[] = [];
  /** Le port de l'agent lui-même (`BR-DEPLOY-007`). */
  agentPort = 7443;
  /** `true` : la consultation échoue (moteur qui ne répond pas) tant que c'est vrai. */
  failCheck = false;
  /** `true` : la lecture des limites de la machine échoue. */
  failCapacity = false;
  private readonly capacities = new Map<string, unknown>();
  private readonly listeners = new Map<string, Map<string, Listener>>();
  private readonly disks = new Map<string, number>();
  private readonly images = new Set<string>();
  private readonly deleting = new Map<string, Set<string>>();
  private readonly held = new Map<string, Map<string, Held>>();
  private nextKey = 1;

  constructor(
    private readonly services: SimulatedServices,
    private readonly ops: SimulatedServiceOps,
    private readonly now: () => number,
  ) {}

  // ── Ce que le serveur sait (pilotable) ───────────────────────────────────────────────────────

  /** Un programme du serveur écoute ce port (`program` : son nom, ou `null` quand l'agent ne peut pas le lire). */
  listen(
    serverId: string,
    port: number,
    program: string | null = null,
    protocol: DeployProtocol = "tcp",
  ): void {
    const bucket = this.listeners.get(serverId) ?? new Map<string, Listener>();
    bucket.set(`${protocol}/${port}`, { program });
    this.listeners.set(serverId, bucket);
  }

  /** L'espace libre du disque des images (octets). Moins de 10 Go : « bas » (`BR-DEPLOY-018`). */
  setDisk(serverId: string, freeBytes: number): void {
    this.disks.set(serverId, freeBytes);
  }

  /** Cette image est déjà sur le serveur : utilisée telle quelle (`dépôt:version`). */
  markPresent(reference: string): void {
    this.images.add(reference);
  }

  /** Un service de ce nom est en cours de suppression (`BR-DEL-019`). */
  markDeleting(serverId: string, name: string): void {
    const bucket = this.deleting.get(serverId) ?? new Set<string>();
    bucket.add(name.toLowerCase());
    this.deleting.set(serverId, bucket);
  }

  /**
   * Les bornes de la machine. `null` rétablit les valeurs d'origine (4 processeurs, 16 Gio) ; un objet les remplace.
   * La forme est celle de `GET /engine/capacity`.
   */
  setCapacity(
    serverId: string,
    host: { cpus: number; memoryBytes: number } | null,
    committedMemoryBytes = 0,
  ): void {
    if (host === null) {
      this.capacities.delete(serverId);
      return;
    }
    const reserve = Math.max(GIB, Math.floor(host.memoryBytes / 20));
    this.capacities.set(serverId, {
      host: { cpus: host.cpus, memory_bytes: host.memoryBytes, reserve_bytes: reserve },
      committed: {
        memory: {
          managed_running: committedMemoryBytes,
          unmanaged_running: 0,
          managed_all: committedMemoryBytes,
          unmanaged_all: 0,
        },
      },
    });
  }

  /** `GET /engine/capacity`, lu comme le lit la coquille. */
  capacity(serverId: string): EngineCapacity | null {
    if (this.failCapacity) throw new Error("capacité illisible");
    const wire = this.capacities.get(serverId) ?? {
      host: { cpus: 4, memory_bytes: 16 * GIB, reserve_bytes: GIB },
      committed: {
        memory: { managed_running: 0, unmanaged_running: 0, managed_all: 0, unmanaged_all: 0 },
      },
    };
    return engineCapacityFromWire(wire);
  }

  // ── La consultation (`POST /deployments/check`) ──────────────────────────────────────────────

  check(serverId: string, input: DeployCheckInput): DeployCheck {
    this.checks.push({ serverId, input: structuredClone(input) });
    return {
      name: this.nameStatus(serverId, input.name),
      invalidFields: this.invalidFields(input),
      ports: this.judgePorts(serverId, input.ports),
      disk: this.disk(serverId),
      imagePresent: this.present(input.image, input.tag),
    };
  }

  private invalidFields(input: DeployCheckInput): string[] {
    const out: string[] = [];
    if (nameIssue(input.name) !== null) out.push("name");
    if (imageIssue(input.image) !== null) out.push("image");
    if (tagIssue(input.tag ?? "", input.image) !== null) out.push("tag");
    return out;
  }

  private nameStatus(serverId: string, name: string): NameStatus {
    if (nameIssue(name) !== null) return "invalid";
    const folded = name.toLowerCase();
    if (this.deleting.get(serverId)?.has(folded)) return "being_deleted";
    const listed = this.services
      .list(serverId)
      .some((service) => service.name.toLowerCase() === folded);
    const running = [...(this.held.get(serverId)?.values() ?? [])].some(
      (held) => held.name.toLowerCase() === folded,
    );
    return listed || running ? "taken" : "ok";
  }

  private disk(serverId: string): { freeBytes: number; low: boolean } {
    const freeBytes = this.disks.get(serverId) ?? 200 * GIB;
    return { freeBytes, low: freeBytes < LOW_DISK_BYTES };
  }

  private present(image: string, tag: string | null): boolean {
    const { repository, embedded } = splitImage(image);
    const version = embedded ?? ((tag ?? "").trim() === "" ? "latest" : (tag ?? "").trim());
    return this.images.has(`${repository}:${version}`);
  }

  /** Qui tient ce port dans le moteur : un service publié (nommé), un conteneur hors Hearth (jamais nommé). */
  private claim(
    serverId: string,
    port: number,
    protocol: DeployProtocol,
  ): { named: string | null; running: boolean } | null {
    let found: { named: string | null; running: boolean } | null = null;
    for (const service of this.services.list(serverId)) {
      if (!service.ports.some((p) => p.hostPort === port && p.protocol === protocol)) continue;
      const running = service.state !== "stopped" && service.state !== "error";
      const published = service.origin === "hearth" || service.adopted !== null;
      const claim = { named: published ? service.name : null, running };
      // Celui qui tourne passe avant celui qui est arrêté.
      if (!found || (claim.running && !found.running)) found = claim;
    }
    return found;
  }

  private verdict(
    serverId: string,
    port: number,
    protocol: DeployProtocol,
    dup: boolean,
  ): { status: PortStatus; holder: string | null } {
    if (port === 0 || dup) return { status: "invalid", holder: null };
    if (protocol === "tcp" && port === this.agentPort)
      return { status: "agent_port", holder: null };
    const listener = this.listeners.get(serverId)?.get(`${protocol}/${port}`);
    const program = () =>
      listener?.program === "docker-proxy"
        ? { status: "held_by_unmanaged" as const, holder: null }
        : { status: "held_by_program" as const, holder: listener?.program ?? null };
    const claim = this.claim(serverId, port, protocol);
    if (claim?.running) {
      return claim.named === null
        ? { status: "held_by_unmanaged", holder: null }
        : { status: "held_by_service", holder: claim.named };
    }
    if (claim) {
      // Un conteneur arrêté réserve le port ; un programme qui l'écoute en plus passe avant.
      if (listener) return program();
      return { status: "reserved_by_stopped", holder: claim.named };
    }
    return listener ? program() : { status: "free", holder: null };
  }

  private suggest(
    serverId: string,
    port: number,
    protocol: DeployProtocol,
    inForm: Set<string>,
  ): number | null {
    const free = (candidate: number): boolean =>
      candidate >= 1 &&
      candidate <= 65535 &&
      !inForm.has(`${protocol}/${candidate}`) &&
      this.verdict(serverId, candidate, protocol, false).status === "free";
    for (let candidate = port + 1; candidate <= Math.min(port + 4096, 65535); candidate += 1) {
      if (free(candidate)) return candidate;
    }
    for (let candidate = port - 1; candidate >= Math.max(port - 4096, 1); candidate -= 1) {
      if (free(candidate)) return candidate;
    }
    return null;
  }

  private judgePorts(serverId: string, ports: readonly DeployPort[]): PortCheck[] {
    const seen = new Set<string>();
    const dups = new Set<string>();
    for (const port of ports) {
      const id = `${port.protocol}/${port.hostPort}`;
      if (seen.has(id)) dups.add(id);
      seen.add(id);
    }
    return ports.map((port) => {
      const id = `${port.protocol}/${port.hostPort}`;
      const found = this.verdict(serverId, port.hostPort, port.protocol, dups.has(id));
      const blocks = found.status !== "free" && found.status !== "reserved_by_stopped";
      const suggested =
        blocks || found.status === "reserved_by_stopped"
          ? this.suggest(serverId, port.hostPort, port.protocol, seen)
          : null;
      return {
        hostPort: port.hostPort,
        protocol: port.protocol,
        status: found.status,
        holder: found.holder,
        suggested,
        privileged: port.hostPort < 1024,
      };
    });
  }

  // ── Le lancement (`POST /deployments`) ───────────────────────────────────────────────────────

  /** Les refus de FORME, dans l'ordre de l'agent (`validate`) : un champ et pourquoi, jamais la valeur. */
  private validate(serverId: string, request: DeployRequest): DeployRefusal | null {
    const invalid = (field: DeployField, reason: string): DeployRefusal => ({
      kind: "invalid",
      field,
      reason,
    });
    if (nameIssue(request.name) !== null) return invalid("name", "bad_format");
    // L'ordre de `ImageRef::parse` : image vide, empreinte, deux versions différentes, image trop longue ou mal
    // formée, puis version mal formée (même une version trop longue est « bad_format »).
    const image = imageIssue(request.image);
    if (image === "required") return invalid("image", "empty");
    if (image === "digest") return invalid("image", "digest_unsupported");
    const tag = tagIssue(request.tag ?? "", request.image);
    if (tag === "conflict") return invalid("tag", "conflict");
    if (image === "too_long") return invalid("image", "too_long");
    if (image === "format") return invalid("image", "bad_format");
    if (tag !== null) return invalid("tag", "bad_format");
    if (request.env.length > MAX_ENV_VARS) return invalid("env", "too_many");
    const names = new Set<string>();
    for (const variable of request.env) {
      if (envNameIssue(variable.name) !== null) return invalid("env", "bad_name");
      if (names.has(variable.name)) return invalid("env", "duplicate_name");
      names.add(variable.name);
      if (envValueIssue(variable.value) !== null) return invalid("env", "bad_value");
    }
    if (request.ports.length > MAX_PORT_MAPPINGS) return invalid("ports", "too_many");
    const ports = new Set<string>();
    for (const port of request.ports) {
      const bad = [port.hostPort, port.containerPort].some(
        (value) => !Number.isInteger(value) || value < 1 || value > 65535,
      );
      if (bad) return invalid("ports", "range");
      const id = `${port.protocol}/${port.hostPort}`;
      if (ports.has(id)) return invalid("ports", "duplicate");
      ports.add(id);
    }
    if (request.command !== null && commandIssue(request.command) !== null) {
      return invalid("command", "bad_format");
    }
    const limits = request.resources;
    if (limits.cpuMillis === null || limits.memoryBytes === null)
      return { kind: "limits_required" };
    const capacity = this.capacity(serverId);
    const cpuMax = capacity ? capacity.cpus * 1000 : Number.POSITIVE_INFINITY;
    const memoryMax = capacity ? capacity.memoryBytes : Number.POSITIVE_INFINITY;
    if (limits.cpuMillis % 250 !== 0) return invalid("resources", "cpu_step");
    if (limits.memoryBytes % (1024 * 1024) !== 0) return invalid("resources", "memory_step");
    if (limits.cpuMillis < MIN_CPU_MILLIS || limits.cpuMillis > cpuMax) {
      return invalid("resources", "cpu_bounds");
    }
    if (limits.memoryBytes < MIN_MEMORY_BYTES || limits.memoryBytes > memoryMax) {
      return invalid("resources", "memory_bounds");
    }
    return null;
  }

  /**
   * Juge et accepte une demande, ou la refuse. Les contrôles dans l'ordre de l'agent : forme, deux déploiements au
   * plus, nom, ports (un port réservé par un service arrêté ne bloque qu'avec la case), place sur le disque. Le droit,
   * la confirmation et le moteur ont déjà été jugés par l'appelant.
   */
  accept(serverId: string, request: DeployRequest, by: string): DeployOutcome {
    const refusal = this.validate(serverId, request);
    if (refusal) return { kind: "refused", refusal };
    const mine = this.held.get(serverId) ?? new Map<string, Held>();
    if (mine.size >= 2) return { kind: "refused", refusal: { kind: "deploy_limit" } };
    const name = this.nameStatus(serverId, request.name);
    if (name === "being_deleted")
      return { kind: "refused", refusal: { kind: "name_being_deleted" } };
    if (name !== "ok") return { kind: "refused", refusal: { kind: "name_taken" } };
    for (const check of this.judgePorts(serverId, request.ports)) {
      const blocks =
        check.status === "reserved_by_stopped"
          ? !request.accept.reservedPorts
          : check.status !== "free";
      if (blocks) {
        return {
          kind: "refused",
          refusal: {
            kind: "port_in_use",
            port: check.hostPort,
            protocol: check.protocol,
            status: check.status,
            holder: check.holder,
            suggested: check.suggested,
          },
        };
      }
    }
    const disk = this.disk(serverId);
    if (disk.low && !request.accept.lowDisk && !this.present(request.image, request.tag)) {
      return { kind: "refused", refusal: { kind: "no_space", freeBytes: disk.freeBytes } };
    }

    const { repository, embedded } = splitImage(request.image);
    const tag =
      embedded ?? ((request.tag ?? "").trim() === "" ? "latest" : (request.tag ?? "").trim());
    const key = `h_SIMDEPLOY${String(this.nextKey++).padStart(6, "0")}`;
    // La ligne « Déploiement en cours » existe avant la première annonce de l'opération (le nom lui est connu).
    this.services.upsert(serverId, {
      key,
      name: request.name,
      origin: "hearth",
      state: "deploying",
      neverStarted: true,
      image: { ref: repository, tag },
      ports: request.ports.map((port) => ({
        hostPort: port.hostPort,
        containerPort: port.containerPort,
        protocol: port.protocol,
        scope: "all" as const,
      })),
    });
    const operation = this.ops.accept(
      serverId,
      {
        key,
        kind: "deploy",
        step: "check",
        steps: [{ step: "check", state: "intent" }],
        cancellable: true,
      },
      by,
    );
    mine.set(operation.id, {
      operationId: operation.id,
      key,
      name: request.name,
      step: "check",
      image: `${repository}:${tag}`,
      by,
    });
    this.held.set(serverId, mine);
    this.received.push({ serverId, request: structuredClone(request) });
    return { kind: "accepted", operationId: operation.id, serviceKey: key };
  }

  // ── La suite de l'opération, pilotée par l'appelant ──────────────────────────────────────────

  private heldOf(serverId: string, opId: string): Held {
    const found = this.held.get(serverId)?.get(opId);
    if (!found) throw new Error(`déploiement inconnu : ${opId}`);
    return found;
  }

  /** La récupération progresse (`percent` : 0 à 100) ; l'étape devient « pull ». */
  progress(
    serverId: string,
    opId: string,
    percent: number | null,
    bytesDone: number | null = null,
    bytesTotal: number | null = null,
  ): void {
    const held = this.heldOf(serverId, opId);
    held.step = "pull";
    this.ops.update(serverId, opId, {
      status: "running",
      step: "pull",
      steps: stepsUpTo("pull"),
      percent,
      bytesDone,
      bytesTotal,
      cancellable: true,
    });
  }

  /** Passe à une étape (« create » est le point de non-retour : l'annulation devient impossible). */
  toStep(serverId: string, opId: string, step: DeployStep): void {
    const held = this.heldOf(serverId, opId);
    held.step = step;
    this.ops.update(serverId, opId, {
      status: "running",
      step,
      steps: stepsUpTo(step),
      cancellable: step === "check" || step === "pull",
    });
  }

  /**
   * Le déploiement réussit : le service est créé (« En marche »). `cause: "stopped_right_after_start"` : le programme
   * s'est arrêté dans les 10 secondes, le service est conservé à l'état `final` (`error` avec `exitCode`, ou
   * `restart_loop`, ou `stopped`).
   */
  succeed(
    serverId: string,
    opId: string,
    after: {
      cause?: "stopped_right_after_start";
      state?: ServiceDto["state"];
      exitCode?: number;
    } = {},
  ): void {
    const held = this.heldOf(serverId, opId);
    const crashed = after.cause === "stopped_right_after_start";
    this.services.upsert(serverId, {
      key: held.key,
      name: held.name,
      state: crashed ? (after.state ?? "error") : "running",
      neverStarted: false,
      exitCode: crashed ? (after.exitCode ?? 1) : null,
      since: new Date(this.now()).toISOString(),
    });
    this.ops.update(serverId, opId, { steps: stepsUpTo("watch", "done") });
    this.ops.finish(serverId, opId, "succeeded", crashed ? "stopped_right_after_start" : null);
    this.release(serverId, opId);
  }

  /** Le déploiement échoue sans rien laisser : aucun service, aucune description (`BR-DEPLOY-019`). */
  fail(
    serverId: string,
    opId: string,
    cause: Parameters<SimulatedServiceOps["finish"]>[3] = "image_not_found",
    status: "failed" | "interrupted" = "failed",
  ): void {
    const held = this.heldOf(serverId, opId);
    // Une interruption ne retire rien : le résultat est inconnu (`BR-DEPLOY-028`).
    if (status === "failed") this.services.remove(serverId, held.key);
    this.ops.finish(serverId, opId, status, cause);
    this.release(serverId, opId);
  }

  private release(serverId: string, opId: string): void {
    this.held.get(serverId)?.delete(opId);
  }

  /** `DELETE /operations/services/{id}` : annule avant le point de non-retour, sinon `too_late`. */
  cancel(serverId: string, opId: string): CancelOutcome {
    this.cancels.push({ serverId, opId });
    const dto = this.ops.of(serverId).find((item) => item.id === opId);
    if (!dto) return { kind: "refused", refusal: { kind: "not_found" } };
    // Idempotent : une opération que l'on annule déjà, ou déjà annulée PAR UN COMPTE, répond la même issue.
    if (
      dto.status === "cancelling" ||
      (dto.status === "cancelled" && dto.cause === "cancelled_by_account")
    ) {
      return { kind: "done", operation: toOperation(dto) };
    }
    if (
      dto.status === "cancelled" ||
      dto.status === "succeeded" ||
      dto.status === "failed" ||
      dto.status === "interrupted"
    ) {
      return { kind: "refused", refusal: { kind: "not_cancellable" } };
    }
    if (!dto.cancellable) return { kind: "refused", refusal: { kind: "too_late" } };
    // Comme l'agent : l'opération passe par « cancelling », puis son exécuteur la clôt « cancelled ».
    // La cause est posée dès cette transition (la clôture ne fait que changer le statut).
    this.ops.update(serverId, opId, {
      status: "cancelling",
      cancellable: false,
      cause: "cancelled_by_account",
    });
    const held = this.held.get(serverId)?.get(opId);
    setTimeout(() => {
      if (held) this.services.remove(serverId, held.key);
      this.ops.finish(serverId, opId, "cancelled", "cancelled_by_account");
      this.release(serverId, opId);
    }, 0);
    const after = this.ops.of(serverId).find((item) => item.id === opId) ?? dto;
    return { kind: "done", operation: toOperation(after) };
  }

  /** Les déploiements en cours d'un serveur (de quoi vérifier). */
  running(
    serverId: string,
  ): { operationId: string; key: string; name: string; step: DeployStep }[] {
    return [...(this.held.get(serverId)?.values() ?? [])].map((held) => ({
      operationId: held.operationId,
      key: held.key,
      name: held.name,
      step: held.step,
    }));
  }
}

/** Les lignes de journal d'un plan arrivé à `step` : les étapes d'avant faites, celle-ci en cours (ou faite). */
function stepsUpTo(step: DeployStep, last: "intent" | "done" = "intent") {
  const index = DEPLOY_STEPS.indexOf(step);
  return DEPLOY_STEPS.slice(0, index + 1).map((name, i) => ({
    step: name,
    state: i < index ? ("done" as const) : last,
  }));
}
