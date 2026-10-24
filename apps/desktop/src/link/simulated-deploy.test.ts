import { describe, expect, it } from "vitest";
import {
  type DeployRefusal,
  type DeployRequest,
  LinkCommandError,
  type ServiceOperationEvent,
  SimulatedLinkBridge,
} from "@/link";

// L'agent simulé juge un déploiement COMME LE VRAI (`application/deployments.rs`, `domain/services/{deploy,ports}.rs`) :
// même ordre de refus, mêmes constats de ports, mêmes bornes. Si ce fichier passait avec un simulateur plus indulgent
// que l'agent, les tests de l'interface prouveraient une fausse chose (HRT-69).
const PASSWORD = "Correct-Horse-9";
const GIB = 1024 ** 3;

function bridge() {
  const sim = new SimulatedLinkBridge({ retryDelayMs: -1 });
  sim.services.seed("forge", []);
  return sim;
}

function request(patch: Partial<DeployRequest> = {}): DeployRequest {
  return {
    name: "web",
    image: "nginx",
    tag: "1.27",
    ports: [],
    env: [],
    command: null,
    resources: { memoryBytes: GIB, cpuMillis: 1000 },
    accept: { reservedPorts: false, lowDisk: false },
    ...patch,
  };
}

async function refused(
  sim: SimulatedLinkBridge,
  patch: Partial<DeployRequest>,
): Promise<DeployRefusal> {
  const outcome = await sim.deployService("forge", request(patch), PASSWORD);
  if (outcome.kind !== "refused") throw new Error(`accepté : ${JSON.stringify(outcome)}`);
  return outcome.refusal;
}

function events(sim: SimulatedLinkBridge) {
  const list: ServiceOperationEvent[] = [];
  void sim.onServiceOperations((event) => list.push(event));
  return list;
}

describe("accepter un déploiement", () => {
  it("répond « accepté » avec l'opération et la clé du service, qui apparaît « Déploiement en cours »", async () => {
    const sim = bridge();
    const outcome = await sim.deployService("forge", request(), PASSWORD);
    expect(outcome.kind).toBe("accepted");
    if (outcome.kind !== "accepted") return;
    expect(outcome.serviceKey).toMatch(/^h_/);
    const listed = sim.services.find("forge", outcome.serviceKey);
    expect(listed).toMatchObject({ name: "web", origin: "hearth", state: "deploying" });
    expect(sim.serviceOps.of("forge")).toMatchObject([
      {
        id: outcome.operationId,
        kind: "deploy",
        status: "running",
        step: "check",
        cancellable: true,
      },
    ]);
    // L'agent garde la demande acceptée (de quoi vérifier ce qui part).
    expect(sim.deploys.received).toHaveLength(1);
  });

  it("le mot de passe est redemandé à chaque fois : l'élévation de cinq minutes ne couvre pas un déploiement (BR-DEPLOY-015)", async () => {
    const sim = bridge();
    // Une première confirmation ouvre l'élévation (créer un compte en lecture seule est couvert).
    expect(await sim.deployService("forge", request(), null)).toMatchObject({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    expect(await sim.deployService("forge", request(), "faux")).toMatchObject({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    sim.reauth.open("forge");
    expect(await sim.deployService("forge", request(), null)).toMatchObject({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    expect((await sim.deployService("forge", request(), PASSWORD)).kind).toBe("accepted");
  });

  it("refuse sans le droit de gérer les services, même avec le bon mot de passe (BR-DEPLOY-001, 030)", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("forge", "marie", false);
    await expect(sim.deployService("forge", request(), PASSWORD)).rejects.toMatchObject({
      failure: { kind: "missing_services_right" },
    });
    await expect(
      sim.checkDeployment("forge", { name: "web", image: "nginx", tag: null, ports: [] }),
    ).rejects.toBeInstanceOf(LinkCommandError);
    expect(sim.deploys.received).toHaveLength(0);
  });

  it("refuse hors « Connecté » sans rien envoyer", async () => {
    const sim = bridge();
    sim.setState("forge", "offline");
    await expect(sim.deployService("forge", request(), PASSWORD)).rejects.toMatchObject({
      failure: { kind: "not_connected" },
    });
  });

  it("refuse quand le moteur n'est pas prêt (BR-DEPLOY-027)", async () => {
    const sim = bridge();
    sim.services.setEngine("forge", { state: "stopped" });
    expect(await refused(sim, {})).toEqual({ kind: "engine_unavailable", engine: "stopped" });
  });

  it("un lien coupé avant la réponse rend « résultat inconnu » et ne rejoue jamais (BR-DEPLOY-028)", async () => {
    const sim = bridge();
    sim.actionMode = "cut";
    const outcome = await sim.deployService("forge", request(), PASSWORD);
    expect(outcome).toMatchObject({ kind: "unknown" });
    // L'agent l'a exécuté avant la coupure : un seul déploiement existe, rien n'est rejoué.
    expect(sim.deploys.received).toHaveLength(1);
    expect(sim.actionMode).toBe("ok");
  });
});

describe("valider comme l'agent (ordre : forme, deux au plus, nom, ports, disque)", () => {
  it("refuse chaque champ avec son code, jamais sa valeur", async () => {
    const sim = bridge();
    const cases: [Partial<DeployRequest>, string, string][] = [
      [{ name: "a" }, "name", "bad_format"],
      [{ name: "web server" }, "name", "bad_format"],
      [{ image: "Nginx" }, "image", "bad_format"],
      [{ image: "" }, "image", "empty"],
      [{ image: "nginx@sha256:abc" }, "image", "digest_unsupported"],
      [{ image: `${"a".repeat(300)}` }, "image", "too_long"],
      [{ tag: "bad tag" }, "tag", "bad_format"],
      [{ image: "nginx:1.27", tag: "1.28" }, "tag", "conflict"],
      // Le conflit de version est jugé AVANT le format de l'image (ordre de `ImageRef::parse`).
      [{ image: "Nginx:1.27", tag: "1.28" }, "tag", "conflict"],
      [{ tag: "a".repeat(129) }, "tag", "bad_format"],
      [{ env: [{ name: "1A", value: "v" }] }, "env", "bad_name"],
      [{ env: [{ name: "A", value: "x\ny" }] }, "env", "bad_value"],
      [
        {
          env: [
            { name: "A", value: "1" },
            { name: "A", value: "2" },
          ],
        },
        "env",
        "duplicate_name",
      ],
      [{ ports: [{ hostPort: 0, containerPort: 80, protocol: "tcp" }] }, "ports", "range"],
      [
        {
          ports: [
            { hostPort: 80, containerPort: 80, protocol: "tcp" },
            { hostPort: 80, containerPort: 81, protocol: "tcp" },
          ],
        },
        "ports",
        "duplicate",
      ],
      [{ command: 'a "b' }, "command", "bad_format"],
      [{ resources: { memoryBytes: GIB, cpuMillis: 125 } }, "resources", "cpu_step"],
      [{ resources: { memoryBytes: GIB, cpuMillis: 0 } }, "resources", "cpu_bounds"],
      [{ resources: { memoryBytes: GIB, cpuMillis: 5000 } }, "resources", "cpu_bounds"],
      [{ resources: { memoryBytes: GIB + 1, cpuMillis: 1000 } }, "resources", "memory_step"],
      [
        { resources: { memoryBytes: 64 * 1024 * 1024, cpuMillis: 1000 } },
        "resources",
        "memory_bounds",
      ],
      [{ resources: { memoryBytes: 17 * GIB, cpuMillis: 1000 } }, "resources", "memory_bounds"],
    ];
    for (const [patch, field, reason] of cases) {
      expect(await refused(sim, patch), JSON.stringify(patch)).toEqual({
        kind: "invalid",
        field,
        reason,
      });
    }
    // Les bornes incluses sont acceptées.
    expect(
      (
        await sim.deployService(
          "forge",
          request({ name: "b1", resources: { memoryBytes: 128 * 1024 * 1024, cpuMillis: 250 } }),
          PASSWORD,
        )
      ).kind,
    ).toBe("accepted");
    expect(
      (
        await sim.deployService(
          "forge",
          request({ name: "b2", resources: { memoryBytes: 16 * GIB, cpuMillis: 4000 } }),
          PASSWORD,
        )
      ).kind,
    ).toBe("accepted");
  });

  it("deux déploiements au plus à la fois, tous comptes confondus (BR-DEPLOY-023)", async () => {
    const sim = bridge();
    expect((await sim.deployService("forge", request({ name: "un" }), PASSWORD)).kind).toBe(
      "accepted",
    );
    expect((await sim.deployService("forge", request({ name: "deux" }), PASSWORD)).kind).toBe(
      "accepted",
    );
    expect(await refused(sim, { name: "trois" })).toEqual({ kind: "deploy_limit" });
  });

  it("un nom pris, sans tenir compte de la casse, par un service ou un déploiement en cours, sans dire par qui (BR-DEPLOY-004)", async () => {
    const sim = bridge();
    sim.services.seed("forge", [
      { key: "c_ext", name: "Autre", origin: "external" },
      { key: "h_mine", name: "base", origin: "hearth" },
    ]);
    expect(await refused(sim, { name: "autre" })).toEqual({ kind: "name_taken" });
    expect(await refused(sim, { name: "BASE" })).toEqual({ kind: "name_taken" });
    expect((await sim.deployService("forge", request({ name: "web" }), PASSWORD)).kind).toBe(
      "accepted",
    );
    expect(await refused(sim, { name: "WEB" })).toEqual({ kind: "name_taken" });
    sim.deploys.markDeleting("forge", "ancien");
    expect(await refused(sim, { name: "Ancien" })).toEqual({ kind: "name_being_deleted" });
  });

  it("les ports : service en marche, conteneur hors Hearth jamais nommé, programme, agent (BR-DEPLOY-007, 008)", async () => {
    const sim = bridge();
    sim.services.seed("forge", [
      {
        key: "h_up",
        name: "up",
        origin: "hearth",
        ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
      },
      {
        key: "c_out",
        name: "dehors",
        origin: "external",
        ports: [{ hostPort: 9000, containerPort: 80, protocol: "tcp", scope: "all" }],
      },
    ]);
    sim.deploys.listen("forge", 5432, "postgres");
    const port = (hostPort: number) => ({
      ports: [{ hostPort, containerPort: 80, protocol: "tcp" as const }],
    });
    expect(await refused(sim, port(8080))).toMatchObject({
      kind: "port_in_use",
      status: "held_by_service",
      holder: "up",
      suggested: 8081,
    });
    expect(await refused(sim, port(9000))).toMatchObject({
      kind: "port_in_use",
      status: "held_by_unmanaged",
      holder: null,
      suggested: 9001,
    });
    expect(await refused(sim, port(5432))).toMatchObject({
      status: "held_by_program",
      holder: "postgres",
    });
    expect(await refused(sim, port(7443))).toMatchObject({ status: "agent_port" });
  });

  it("un port réservé par un service arrêté ne bloque qu'avec la case cochée (BR-DEPLOY-008)", async () => {
    const sim = bridge();
    sim.services.seed("forge", [
      {
        key: "h_off",
        name: "arrete",
        origin: "hearth",
        state: "stopped",
        ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
      },
    ]);
    const ports = [{ hostPort: 8080, containerPort: 80, protocol: "tcp" as const }];
    expect(await refused(sim, { ports })).toMatchObject({
      status: "reserved_by_stopped",
      holder: "arrete",
    });
    expect(
      (
        await sim.deployService(
          "forge",
          request({ ports, accept: { reservedPorts: true, lowDisk: false } }),
          PASSWORD,
        )
      ).kind,
    ).toBe("accepted");
  });

  it("propose le plus petit port libre au-dessus, sinon le plus grand en dessous (BR-DEPLOY-009)", async () => {
    const sim = bridge();
    const taken = (hostPort: number) => ({
      hostPort,
      containerPort: 80,
      protocol: "tcp" as const,
      scope: "all" as const,
    });
    sim.services.seed("forge", [
      { key: "h_a", name: "a", origin: "hearth", ports: [taken(8080), taken(8081)] },
      { key: "h_b", name: "b", origin: "hearth", ports: [taken(65535)] },
    ]);
    const answer = await sim.checkDeployment("forge", {
      name: "web",
      image: "nginx",
      tag: null,
      ports: [
        { hostPort: 8080, containerPort: 80, protocol: "tcp" },
        { hostPort: 65535, containerPort: 80, protocol: "tcp" },
        { hostPort: 8082, containerPort: 80, protocol: "tcp" },
      ],
    });
    // 8082 est dans le formulaire : jamais proposé pour 8080.
    expect(answer.ports.map((port) => [port.hostPort, port.status, port.suggested])).toEqual([
      [8080, "held_by_service", 8083],
      [65535, "held_by_service", 65534],
      [8082, "free", null],
    ]);
  });

  it("la place sur le disque : moins de 10 Go libres refuse sans la case, sauf image déjà présente (BR-DEPLOY-018)", async () => {
    const sim = bridge();
    sim.deploys.setDisk("forge", 5 * GIB);
    expect(await refused(sim, {})).toEqual({ kind: "no_space", freeBytes: 5 * GIB });
    expect(
      (
        await sim.deployService(
          "forge",
          request({ name: "ok", accept: { reservedPorts: false, lowDisk: true } }),
          PASSWORD,
        )
      ).kind,
    ).toBe("accepted");
    sim.deploys.markPresent("redis:7");
    expect(
      (
        await sim.deployService(
          "forge",
          request({ name: "present", image: "redis", tag: "7" }),
          PASSWORD,
        )
      ).kind,
    ).toBe("accepted");
  });
});

describe("la consultation (POST /deployments/check)", () => {
  it("dit le nom, l'image présente, l'espace, et ne reçoit jamais un secret", async () => {
    const sim = bridge();
    sim.deploys.markPresent("nginx:latest");
    const answer = await sim.checkDeployment("forge", {
      name: "web",
      image: "nginx",
      tag: null,
      ports: [],
    });
    expect(answer).toEqual({
      name: "ok",
      invalidFields: [],
      ports: [],
      disk: { freeBytes: 200 * GIB, low: false },
      imagePresent: true,
    });
    expect(JSON.stringify(sim.deploys.checks)).not.toMatch(/value|command|env/);
    const bad = await sim.checkDeployment("forge", {
      name: "x",
      image: "Nginx",
      tag: null,
      ports: [],
    });
    expect(bad.name).toBe("invalid");
    expect(bad.invalidFields).toEqual(["name", "image"]);
  });

  it("n'est pas un acte : aucune confirmation, aucune opération", async () => {
    const sim = bridge();
    await sim.checkDeployment("forge", { name: "web", image: "nginx", tag: null, ports: [] });
    expect(sim.serviceOps.of("forge")).toHaveLength(0);
    expect(sim.calls).toContain("deploy check");
  });
});

describe("la suite de l'opération et l'annulation (BR-DEPLOY-016, 019)", () => {
  async function started(sim: SimulatedLinkBridge) {
    const outcome = await sim.deployService("forge", request(), PASSWORD);
    if (outcome.kind !== "accepted") throw new Error("refusé");
    return outcome;
  }

  it("la récupération progresse, la création est le point de non-retour", async () => {
    const sim = bridge();
    const { operationId } = await started(sim);
    sim.deploys.progress("forge", operationId, 40, 400, 1000);
    expect(sim.serviceOps.of("forge")[0]).toMatchObject({
      step: "pull",
      percent: 40,
      bytesDone: 400,
      bytesTotal: 1000,
      cancellable: true,
    });
    sim.deploys.toStep("forge", operationId, "create");
    expect(sim.serviceOps.of("forge")[0]).toMatchObject({ step: "create", cancellable: false });
  });

  it("annuler pendant la récupération ne laisse aucun service ; après la création : trop tard", async () => {
    const sim = bridge();
    const first = await started(sim);
    sim.deploys.progress("forge", first.operationId, 10);
    const seen: string[] = [];
    void sim.onServiceOperations((event) => {
      if (event.kind === "upsert") seen.push(event.operation.status);
    });
    const done = await sim.cancelServiceOperation("forge", first.operationId, PASSWORD);
    // Comme l'agent : l'opération PASSE PAR « cancelling », son exécuteur la clôt ensuite « cancelled ».
    expect(done).toMatchObject({
      kind: "done",
      operation: { status: "cancelling", cause: "cancelled_by_account" },
    });
    expect(sim.services.find("forge", first.serviceKey)?.state).toBe("deploying");
    // Idempotent : une seconde demande pendant l'annulation répond la même chose.
    expect(await sim.cancelServiceOperation("forge", first.operationId, PASSWORD)).toMatchObject({
      kind: "done",
      operation: { status: "cancelling" },
    });
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(seen).toEqual(["running", "cancelling", "cancelled"]);
    expect(sim.services.find("forge", first.serviceKey)).toBeUndefined();
    // Annulée PAR UN COMPTE : même issue ; annulée pour une autre cause (délai dépassé) : plus annulable.
    expect(await sim.cancelServiceOperation("forge", first.operationId, PASSWORD)).toMatchObject({
      kind: "done",
      operation: { status: "cancelled", cause: "cancelled_by_account" },
    });

    const second = await sim.deployService("forge", request({ name: "deux" }), PASSWORD);
    if (second.kind !== "accepted") throw new Error("refusé");
    sim.deploys.toStep("forge", second.operationId, "create");
    expect(await sim.cancelServiceOperation("forge", second.operationId, PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "too_late" },
    });
    expect(await sim.cancelServiceOperation("forge", "inconnue", PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "not_found" },
    });
  });

  it("une opération annulée pour une autre cause qu'un compte n'est plus annulable", async () => {
    const sim = bridge();
    const operation = sim.serviceOps.accept("forge", { key: "h_x", kind: "deploy" });
    sim.serviceOps.finish("forge", operation.id, "cancelled", "decision_expired");
    expect(await sim.cancelServiceOperation("forge", operation.id, PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "not_cancellable" },
    });
  });

  it("annuler est couvert par l'élévation : sans élévation, le mot de passe est exigé", async () => {
    const sim = bridge();
    const { operationId } = await started(sim);
    // Le mot de passe du déploiement vient d'ouvrir l'élévation : on la ferme pour juger l'annulation seule.
    sim.reauth.close("forge");
    expect(await sim.cancelServiceOperation("forge", operationId, null)).toMatchObject({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    sim.reauth.open("forge");
    expect((await sim.cancelServiceOperation("forge", operationId, null)).kind).toBe("done");
  });

  it("un échec retire le service, un succès le met en marche, un crash le garde en erreur", async () => {
    const sim = bridge();
    const list = events(sim);
    const failed = await started(sim);
    sim.deploys.fail("forge", failed.operationId, "tag_not_found");
    expect(sim.services.find("forge", failed.serviceKey)).toBeUndefined();

    const ok = await sim.deployService("forge", request({ name: "ok" }), PASSWORD);
    if (ok.kind !== "accepted") throw new Error("refusé");
    sim.deploys.succeed("forge", ok.operationId);
    expect(sim.services.find("forge", ok.serviceKey)).toMatchObject({ state: "running" });

    const crash = await sim.deployService("forge", request({ name: "crash" }), PASSWORD);
    if (crash.kind !== "accepted") throw new Error("refusé");
    sim.deploys.succeed("forge", crash.operationId, {
      cause: "stopped_right_after_start",
      exitCode: 137,
    });
    expect(sim.services.find("forge", crash.serviceKey)).toMatchObject({
      state: "error",
      exitCode: 137,
    });

    const statuses = list
      .filter((event) => event.kind === "upsert")
      .map((event) =>
        event.kind === "upsert" ? [event.operation.id, event.operation.status] : [],
      );
    expect(statuses.filter(([id]) => id === failed.operationId).at(-1)).toEqual([
      failed.operationId,
      "failed",
    ]);
    expect(statuses.filter(([id]) => id === crash.operationId).at(-1)).toEqual([
      crash.operationId,
      "succeeded",
    ]);
  });

  it("les bornes de la machine se lisent comme l'agent les donne ; illisibles : null", async () => {
    const sim = bridge();
    expect(await sim.readEngineCapacity("forge")).toEqual({
      cpus: 4,
      memoryBytes: 16 * GIB,
      reserveBytes: GIB,
      committedMemoryBytes: 0,
    });
    sim.deploys.setCapacity("forge", { cpus: 8, memoryBytes: 64 * GIB }, 2 * GIB);
    expect(await sim.readEngineCapacity("forge")).toEqual({
      cpus: 8,
      memoryBytes: 64 * GIB,
      reserveBytes: Math.floor((64 * GIB) / 20),
      committedMemoryBytes: 2 * GIB,
    });
  });
});
