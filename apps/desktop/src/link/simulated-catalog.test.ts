import { describe, expect, it } from "vitest";
import {
  type DeployRefusal,
  type FicheDetail,
  type FicheInstallRequest,
  LinkCommandError,
  SimulatedLinkBridge,
} from "@/link";

// L'agent simulé juge une installation depuis une fiche COMME LE VRAI (`application/fiche_install.rs`,
// `domain/services/from_fiche.rs`) : même ordre de refus, mêmes codes (FICHE_UNKNOWN, FICHE_CHANGED, AGREEMENT_REQUIRED,
// SETTING_REFUSED, HOST_TOO_SMALL, NO_SPACE). Si ce fichier passait avec un simulateur plus indulgent que l'agent, les
// tests de l'interface prouveraient une fausse chose (HRT-102). Les valeurs viennent des fiches simulées : Terraria a
// la forme réelle de `GET /catalog/terraria`, la fiche d'essai porte tout ce qu'une fiche peut offrir.
const PASSWORD = "Correct-Horse-9";
const GIB = 1024 ** 3;

function bridge() {
  const sim = new SimulatedLinkBridge({ retryDelayMs: -1 });
  sim.services.seed("forge", []);
  return sim;
}

async function detailOf(sim: SimulatedLinkBridge, id: string): Promise<FicheDetail> {
  const read = await sim.readFiche("forge", id);
  if (read.kind !== "found") throw new Error("fiche absente");
  return read.fiche;
}

function terrariaRequest(
  detail: FicheDetail,
  patch: Partial<FicheInstallRequest> = {},
): FicheInstallRequest {
  return {
    name: "mon-terraria",
    fiche: detail.card.id,
    version: detail.card.version,
    digest: detail.digest,
    variant: null,
    answers: [{ id: "world_name", value: "Monde" }],
    agreements: [],
    ports: [{ id: "game", hostPort: 7778 }],
    optionalPorts: [],
    resources: { memoryBytes: 2 * GIB, cpuMillis: 1000 },
    accept: { reservedPorts: false, lowDisk: false },
    ...patch,
  };
}

async function refused(
  sim: SimulatedLinkBridge,
  request: FicheInstallRequest,
): Promise<DeployRefusal> {
  const outcome = await sim.installFiche("forge", request, PASSWORD);
  if (outcome.kind !== "refused") throw new Error(`accepté : ${JSON.stringify(outcome)}`);
  return outcome.refusal;
}

describe("lire le catalogue", () => {
  it("liste Terraria avec la forme réelle d'une fiche, et son détail porte tout ce que la fiche offre", async () => {
    const sim = bridge();
    const catalog = await sim.readCatalog("forge");
    expect(catalog.fiches.map((card) => card.id)).toEqual(["terraria"]);
    expect(catalog.refused).toEqual([]);
    const detail = await detailOf(sim, "terraria");
    expect(detail.card).toMatchObject({
      name: "Terraria",
      version: 3,
      status: "preview",
      installable: true,
    });
    expect(detail.digest).toMatch(/^[0-9a-f]{64}$/);
    expect(detail.image).toEqual({
      repository: "ryshe/terraria",
      tag: "vanilla-1.4.5.8",
      digest: null,
    });
    expect(detail.questions.map((question) => question.id)).toEqual([
      "world_name",
      "password",
      "motd",
      "max_players",
      "world_size",
      "difficulty",
    ]);
    expect(detail.questions.length).toBeLessThanOrEqual(8);
    // Toutes les ressources sont « non mesurées » : aucun nombre à côté.
    for (const need of [detail.resources.cpu, detail.resources.memory]) {
      expect(need.evidence).toEqual({ kind: "unmeasured" });
    }
    expect(detail.resources.memory.minBytes).toBeNull();
    // Aucune ligne « compte exigé » n'existe dans la fiche.
    expect(JSON.stringify(detail)).not.toMatch(/compte exig|account/i);
  });

  it("lit une fiche inconnue comme « inconnue », jamais comme une panne", async () => {
    const sim = bridge();
    expect(await sim.readFiche("forge", "nope")).toEqual({ kind: "unknown" });
    sim.catalog.failRead = true;
    await expect(sim.readCatalog("forge")).rejects.toBeInstanceOf(LinkCommandError);
  });

  it("dit les fiches écartées et celles que ce client ne sait pas lire", async () => {
    const sim = bridge();
    sim.catalog.refused = [{ dir: "autre", invariant: "I2" }];
    sim.catalog.unreadable = 2;
    const catalog = await sim.readCatalog("forge");
    expect(catalog.refused).toEqual([{ dir: "autre", invariant: "I2" }]);
    expect(catalog.unreadable).toBe(2);
  });
});

describe("accepter une installation", () => {
  it("accepte une demande juste : le service apparaît « Déploiement en cours » et l'agent garde la demande", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    const outcome = await sim.installFiche("forge", terrariaRequest(detail), PASSWORD);
    expect(outcome.kind).toBe("accepted");
    if (outcome.kind !== "accepted") return;
    expect(sim.services.find("forge", outcome.serviceKey)).toMatchObject({
      name: "mon-terraria",
      origin: "hearth",
      state: "deploying",
    });
    expect(sim.catalog.received).toHaveLength(1);
    expect(sim.serviceOps.of("forge")).toMatchObject([{ kind: "deploy", status: "running" }]);
  });

  it("exige le mot de passe à chaque fois : jamais couvert par l'élévation", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    const outcome = await sim.installFiche("forge", terrariaRequest(detail), "faux");
    expect(outcome).toMatchObject({ kind: "refused", refusal: { kind: "wrong_password" } });
    expect(sim.catalog.received).toHaveLength(0);
  });
});

describe("refuser comme l'agent, dans l'ordre", () => {
  it("FICHE_UNKNOWN : une fiche que l'agent ne propose pas", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    expect(await refused(sim, terrariaRequest(detail, { fiche: "nope" }))).toEqual({
      kind: "fiche_unknown",
    });
  });

  it("FICHE_CHANGED : une version ou une empreinte différentes de celles que le client a lues", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    expect(await refused(sim, terrariaRequest(detail, { version: 1 }))).toEqual({
      kind: "fiche_changed",
    });
    expect(await refused(sim, terrariaRequest(detail, { digest: "0".repeat(64) }))).toEqual({
      kind: "fiche_changed",
    });
    sim.catalog.bump("terraria");
    expect(await refused(sim, terrariaRequest(detail))).toEqual({ kind: "fiche_changed" });
  });

  it("SETTING_REFUSED : la question et la raison, au plus UN caractère, jamais la valeur", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    const bad = (id: string, value: string) =>
      refused(sim, terrariaRequest(detail, { answers: [{ id, value }] }));
    expect(await bad("world_name", "Mon monde")).toEqual({
      kind: "answer_refused",
      id: "world_name",
      reason: "charset",
      character: " ",
    });
    expect(await bad("max_players", "300")).toMatchObject({ reason: "bounds", character: null });
    expect(await bad("max_players", "huit")).toMatchObject({ reason: "bounds" });
    expect(await bad("world_size", "9")).toMatchObject({ reason: "choice" });
    expect(await bad("world_name", "a".repeat(101))).toMatchObject({ reason: "length" });
    expect(await bad("password", "a\nb")).toMatchObject({ reason: "charset", character: null });
    // Un identifiant inconnu vient du client : il ne revient pas.
    expect(await bad("teleport", "x")).toEqual({
      kind: "answer_refused",
      id: null,
      reason: "unknown_id",
      character: null,
    });
    expect(sim.catalog.received).toHaveLength(0);
  });

  it("AGREEMENT_REQUIRED : même d'un client modifié, avec les contrats qui manquent", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    const request: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "server_token", value: "jeton" }],
      ports: [{ id: "game", hostPort: 25570 }],
    };
    expect(await refused(sim, request)).toEqual({ kind: "agreement_required", ids: ["licence"] });
    const outcome = await sim.installFiche(
      "forge",
      { ...request, agreements: ["licence"] },
      PASSWORD,
    );
    expect(outcome.kind).toBe("accepted");
  });

  it("refuse une réponse d'une autre variante, un port sous 1024 et un port publié sans numéro", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    const base: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "server_token", value: "jeton" }],
      agreements: ["licence"],
      ports: [{ id: "game", hostPort: 25570 }],
    };
    expect(
      await refused(sim, {
        ...base,
        answers: [...base.answers, { id: "mod_pack", value: "base" }],
      }),
    ).toMatchObject({ kind: "answer_refused", id: "mod_pack", reason: "variant" });
    expect(await refused(sim, { ...base, ports: [{ id: "game", hostPort: 80 }] })).toEqual({
      kind: "other",
    });
    expect(await refused(sim, { ...base, ports: [] })).toEqual({ kind: "other" });
    expect(await refused(sim, { ...base, variant: "inconnue" })).toEqual({ kind: "other" });
  });

  it("LIMITS_REQUIRED et bornes de la machine, comme un déploiement ordinaire", async () => {
    const sim = bridge();
    sim.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 16 * GIB });
    const detail = await detailOf(sim, "terraria");
    expect(
      await refused(
        sim,
        terrariaRequest(detail, { resources: { memoryBytes: 512 * 1024 ** 2, cpuMillis: 100 } }),
      ),
    ).toMatchObject({ kind: "invalid", field: "resources", reason: "cpu_step" });
    expect(
      await refused(
        sim,
        terrariaRequest(detail, { resources: { memoryBytes: 64 * GIB, cpuMillis: 1000 } }),
      ),
    ).toMatchObject({ kind: "invalid", field: "resources", reason: "memory_bounds" });
  });

  it("HOST_TOO_SMALL : seulement un minimum MESURÉ, jamais le chiffre de l'éditeur", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    sim.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 1 * GIB });
    const request: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "server_token", value: "jeton" }],
      agreements: ["licence"],
      ports: [{ id: "game", hostPort: 25570 }],
      resources: { memoryBytes: 512 * 1024 ** 2, cpuMillis: 1000 },
    };
    expect(await refused(sim, request)).toEqual({
      kind: "host_too_small",
      lack: "memory",
      needed: 2 * GIB,
      has: 1 * GIB,
    });
    // Le processeur mesuré : 1 processeur au minimum, la machine n'en a pas assez.
    sim.deploys.setCapacity("forge", { cpus: 1, memoryBytes: 16 * GIB });
    const cpuMin = sim.catalog.get("essai") as FicheDetail;
    cpuMin.resources.cpu.minMillis = 2000;
    expect(
      await refused(sim, { ...request, resources: { memoryBytes: 4 * GIB, cpuMillis: 1000 } }),
    ).toEqual({
      kind: "host_too_small",
      lack: "cpu",
      needed: 2000,
      has: 1000,
    });
    // La machine suffit : l'installation est acceptée.
    sim.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 16 * GIB });
    const outcome = await sim.installFiche(
      "forge",
      { ...request, resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 } },
      PASSWORD,
    );
    expect(outcome.kind).toBe("accepted");
  });

  it("NO_SPACE : un besoin mesuré refuse sans case, celui de l'éditeur demande la case", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    const request: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "server_token", value: "jeton" }],
      agreements: ["licence"],
      ports: [{ id: "game", hostPort: 25570 }],
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
    };
    sim.deploys.setDisk("forge", 2 * GIB);
    // Le besoin annoncé par l'éditeur (8 Gio) dépasse la place : la case « Continuer malgré le manque de place ».
    expect(await refused(sim, request)).toEqual({ kind: "no_space", freeBytes: 2 * GIB });
    // Le besoin MESURÉ : aucune case ne passe outre.
    const measured = sim.catalog.get("essai") as FicheDetail;
    measured.resources.disk.evidence = { kind: "measured" };
    expect(
      await refused(sim, { ...request, accept: { reservedPorts: false, lowDisk: true } }),
    ).toEqual({
      kind: "no_space_to_install",
      neededBytes: 8 * GIB,
      freeBytes: 2 * GIB,
      missingBytes: 6 * GIB,
    });
  });

  it("refuse comme un déploiement : nom pris, port tenu, deux déploiements au plus", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    sim.services.seed("forge", [
      { key: "h_x", name: "mon-terraria", origin: "hearth", state: "running" },
    ]);
    expect(await refused(sim, terrariaRequest(detail))).toEqual({ kind: "name_taken" });
    // Un programme du serveur tient déjà le port : l'agent propose le port libre le plus proche.
    sim.deploys.listen("forge", 7778, "sshd");
    expect(await refused(sim, terrariaRequest(detail, { name: "autre" }))).toMatchObject({
      kind: "port_in_use",
      port: 7778,
      status: "held_by_program",
      suggested: 7779,
    });
    // Un port réservé par un service arrêté demande sa case.
    sim.services.seed("forge", [
      {
        key: "h_stopped",
        name: "vieux",
        origin: "hearth",
        state: "stopped",
        ports: [{ hostPort: 7790, containerPort: 7777, protocol: "tcp", scope: "all" }],
      },
    ]);
    const reserved = terrariaRequest(detail, {
      name: "autre",
      ports: [{ id: "game", hostPort: 7790 }],
    });
    expect(await refused(sim, reserved)).toMatchObject({
      kind: "port_in_use",
      status: "reserved_by_stopped",
    });
    const accepted = await sim.installFiche(
      "forge",
      { ...reserved, accept: { reservedPorts: true, lowDisk: false } },
      PASSWORD,
    );
    expect(accepted.kind).toBe("accepted");
    // Deux déploiements au plus à la fois.
    const otherPort = (name: string, hostPort: number) =>
      terrariaRequest(detail, { name, ports: [{ id: "game", hostPort }] });
    const second = await sim.installFiche("forge", otherPort("deux", 7800), PASSWORD);
    expect(second.kind).toBe("accepted");
    expect(await refused(sim, otherPort("trois", 7801))).toEqual({ kind: "deploy_limit" });
  });

  it("compte « mesuré au repos » comme mesuré pour le processeur aussi, comme l'agent", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    rich.resources.cpu = {
      minMillis: 2000,
      recommendedMillis: null,
      evidence: { kind: "measured_idle" },
    };
    sim.catalog.put(rich);
    sim.deploys.setCapacity("forge", { cpus: 1, memoryBytes: 16 * GIB });
    const request: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "server_token", value: "jeton" }],
      agreements: ["licence"],
      ports: [{ id: "game", hostPort: 25570 }],
      resources: { memoryBytes: 4 * GIB, cpuMillis: 1000 },
    };
    expect(await refused(sim, request)).toEqual({
      kind: "host_too_small",
      lack: "cpu",
      needed: 2000,
      has: 1000,
    });
  });

  it("juge les contrats avant les réponses : un contrat manquant et une réponse fausse disent le contrat", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    const request: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "vanilla",
      answers: [{ id: "slots", value: "9999" }],
      ports: [{ id: "game", hostPort: 25570 }],
    };
    expect(await refused(sim, request)).toEqual({ kind: "agreement_required", ids: ["licence"] });
  });

  it("répond « autre refus » pour une fiche sans serveur sur ce système : l'agent répond sur `source`, pas sur le nom", async () => {
    const sim = bridge();
    const detail = await detailOf(sim, "terraria");
    const unavailable = sim.catalog.get("terraria");
    if (unavailable) unavailable.card.installable = false;
    expect(await refused(sim, terrariaRequest(detail))).toEqual({ kind: "other" });
  });

  it("ne publie que les ports de la variante, et donne au service le port du serveur quand le jeu annonce le sien", async () => {
    const sim = bridge();
    const rich = sim.catalog.addRich();
    const base: FicheInstallRequest = {
      ...terrariaRequest(rich),
      variant: "modded",
      answers: [
        { id: "server_token", value: "jeton" },
        { id: "mod_pack", value: "base" },
      ],
      agreements: ["licence"],
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
    };
    // Le port de la variante avec mods manque : refus.
    expect(await refused(sim, { ...base, ports: [{ id: "game", hostPort: 25570 }] })).toEqual({
      kind: "other",
    });
    // Un port d'une autre variante envoyé tel quel : refus.
    expect(
      await refused(sim, {
        ...base,
        variant: "vanilla",
        answers: [{ id: "server_token", value: "jeton" }],
        ports: [
          { id: "game", hostPort: 25570 },
          { id: "mods_port", hostPort: 7778 },
        ],
      }),
    ).toEqual({ kind: "other" });
    const outcome = await sim.installFiche(
      "forge",
      {
        ...base,
        ports: [
          { id: "game", hostPort: 25570 },
          { id: "mods_port", hostPort: 7778 },
        ],
      },
      PASSWORD,
    );
    expect(outcome.kind).toBe("accepted");
    if (outcome.kind !== "accepted") return;
    expect(sim.services.find("forge", outcome.serviceKey)?.ports).toMatchObject([
      { hostPort: 25570, containerPort: 25565 },
      { hostPort: 7778, containerPort: 7778 },
    ]);
  });
});
