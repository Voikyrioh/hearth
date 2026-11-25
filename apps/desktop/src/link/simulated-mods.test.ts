import { describe, expect, it } from "vitest";
import type { ModsBatch } from "./mods";
import { SimulatedLinkBridge } from "./simulated";
import { simMod } from "./simulated-mods";

// L'agent simulé des mods juge COMME LE VRAI (`crates/hearth-agent/src/application/mods.rs`) : un test qui passe ici
// passerait contre l'agent. Le droit et le mot de passe (jamais couvert par l'élévation) sont jugés avant.
const PASSWORD = "Correct-Horse-9";
const EMPTY: ModsBatch = { add: [], retire: [], restore: [], disable: [], enable: [] };

function identity(id: string, version: string, loader = "fabric") {
  return {
    id,
    name: id,
    version,
    loader,
    minecraftDeclared: true,
    environment: "both" as const,
  };
}

function bridge() {
  const sim = new SimulatedLinkBridge();
  sim.services.seed("forge", [
    { key: "h_mc", name: "Prominence", origin: "hearth" },
    { key: "h_vanilla", name: "Vanilla", origin: "hearth" },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
  ]);
  sim.mods.seed("forge", "h_mc", {
    mods: [
      simMod({ file: "lithium-0.11.2.jar", identity: identity("lithium", "0.11.2") }),
      simMod({ file: "pack.jar", origin: "modrinth_pack", identity: identity("pack", "1.0") }),
      simMod({ file: "carpet.jar.disabled", identity: identity("carpet", "1.4") }),
    ],
  });
  sim.mods.seed("forge", "h_vanilla", { applicable: "no_mods" });
  return sim;
}

describe("la liste des mods", () => {
  it("rend la liste posée, et un service inconnu ou adopté est refusé par son code", async () => {
    const sim = bridge();
    const outcome = await sim.readMods("forge", "h_mc");
    expect(outcome.kind).toBe("ready");
    if (outcome.kind !== "ready") return;
    expect(outcome.view.mods.map((mod) => mod.file)).toEqual([
      "lithium-0.11.2.jar",
      "pack.jar",
      "carpet.jar.disabled",
    ]);
    expect(outcome.view.mods[2]?.enabled).toBe(false);
    expect(await sim.readMods("forge", "h_nope")).toEqual({
      kind: "refused",
      refusal: { kind: "service_not_found" },
    });
    expect(await sim.readMods("forge", "e_old")).toEqual({
      kind: "refused",
      refusal: { kind: "service_not_found" },
    });
  });

  it("un gros serveur se lit en plusieurs fois : `unread` baisse à chaque lecture", async () => {
    const sim = bridge();
    sim.mods.unreadSteps = [30, 10, 0];
    const reads: number[] = [];
    for (let i = 0; i < 3; i += 1) {
      const outcome = await sim.readMods("forge", "h_mc");
      reads.push(outcome.kind === "ready" ? outcome.view.unread : -1);
    }
    expect(reads).toEqual([30, 10, 0]);
  });
});

describe("le contrôle des dépôts", () => {
  it("ajoute, remplace, refuse un autre moteur et refuse une version déjà installée", async () => {
    const sim = bridge();
    sim.mods.registerDeposit("D1", { identity: identity("chunky", "1.3"), sha256: "a1" });
    sim.mods.registerDeposit("D2", { identity: identity("lithium", "0.11.4"), sha256: "a2" });
    sim.mods.registerDeposit("D3", { identity: identity("create", "0.5", "forge"), sha256: "a3" });
    sim.mods.registerDeposit("D4", { identity: identity("lithium", "0.11.2"), sha256: "a4" });
    const outcome = await sim.checkMods("forge", "h_mc", ["D1", "D2", "D3"]);
    expect(outcome.kind).toBe("ready");
    if (outcome.kind !== "ready") return;
    const [add, replace, wrong] = outcome.files;
    expect(add).toMatchObject({ outcome: "add", targetFile: "chunky-1.3.jar" });
    expect(replace).toMatchObject({
      outcome: "replace",
      replaces: { file: "lithium-0.11.2.jar", version: "0.11.2" },
    });
    expect(wrong).toMatchObject({
      outcome: "refused",
      refusal: "MOD_INCOMPATIBLE_LOADER",
      refusalValues: { loader: "forge", serverLoader: "fabric" },
    });
    const alone = await sim.checkMods("forge", "h_mc", ["D4"]);
    expect(alone).toMatchObject({
      files: [{ outcome: "refused", refusal: "MOD_SAME_VERSION" }],
    });
    // Deux fois le même mod dans un lot : les deux sont refusés.
    const twice = await sim.checkMods("forge", "h_mc", ["D2", "D4"]);
    expect(twice).toMatchObject({
      files: [{ refusal: "MOD_DUPLICATE_IN_BATCH" }, { refusal: "MOD_DUPLICATE_IN_BATCH" }],
    });
  });

  it("un dépôt inconnu arrête tout le contrôle ; un moteur sans mods le refuse", async () => {
    const sim = bridge();
    expect(await sim.checkMods("forge", "h_mc", ["X"])).toEqual({
      kind: "refused",
      refusal: { kind: "deposit_unknown" },
    });
    expect(await sim.checkMods("forge", "h_vanilla", ["X"])).toEqual({
      kind: "refused",
      refusal: { kind: "not_applicable" },
    });
  });
});

describe("l'application d'un lot", () => {
  it("exige le mot de passe à chaque fois, même juste après l'avoir donné", async () => {
    const sim = bridge();
    const first = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, disable: ["lithium-0.11.2.jar"] },
      PASSWORD,
    );
    expect(first.kind).toBe("accepted");
    if (first.kind === "accepted") sim.mods.succeed("forge", first.operationId);
    const second = await sim.applyMods("forge", "h_mc", { ...EMPTY, retire: ["pack.jar"] }, null);
    expect(second).toEqual({
      kind: "refused",
      refusal: { kind: "common", refusal: { kind: "password_required" } },
    });
  });

  it("refuse comme l'agent : lot hors bornes, fichier absent, mod du pack Modrinth, déjà actif", async () => {
    const sim = bridge();
    const code = async (batch: ModsBatch) => {
      const outcome = await sim.applyMods("forge", "h_mc", batch, PASSWORD);
      return outcome.kind === "refused" && outcome.refusal.kind === "mod"
        ? outcome.refusal.code
        : outcome.kind;
    };
    expect(await code(EMPTY)).toBe("MODS_BATCH_INVALID");
    expect(await code({ ...EMPTY, retire: ["absent.jar"] })).toBe("MOD_FILE_MISSING");
    expect(await code({ ...EMPTY, disable: ["pack.jar"] })).toBe("MOD_IN_MODRINTH_PACK");
    sim.mods
      .viewOf("forge", "h_mc")
      ?.mods.push(simMod({ file: "carpet-2.jar", identity: identity("carpet", "2.0") }));
    expect(await code({ ...EMPTY, enable: ["carpet.jar.disabled"] })).toBe("MOD_ALREADY_ACTIVE");
    const vanilla = await sim.applyMods(
      "forge",
      "h_vanilla",
      { ...EMPTY, retire: ["a.jar"] },
      PASSWORD,
    );
    expect(vanilla).toMatchObject({ refusal: { kind: "mod", code: "MODS_NOT_APPLICABLE" } });
  });

  it("un dépôt changé depuis le contrôle est refusé, et une opération en cours refuse la suivante", async () => {
    const sim = bridge();
    sim.mods.registerDeposit("D1", { identity: identity("chunky", "1.3"), sha256: "a1" });
    const changed = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, add: [{ deposit: "D1", sha256: "autre", replace: null }] },
      PASSWORD,
    );
    expect(changed).toMatchObject({
      refusal: { kind: "mod", code: "MOD_REFUSED", deposit: "D1" },
    });
    const first = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, retire: ["lithium-0.11.2.jar"] },
      PASSWORD,
    );
    expect(first.kind).toBe("accepted");
    const second = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, disable: ["carpet.jar.disabled"] },
      PASSWORD,
    );
    expect(second).toMatchObject({ refusal: { kind: "mod", code: "OPERATION_IN_PROGRESS" } });
  });

  it("la mise en place change la liste : ajout, remplacement, réactivation", async () => {
    const sim = bridge();
    sim.mods.registerDeposit("D2", { identity: identity("lithium", "0.11.4"), sha256: "a2" });
    const outcome = await sim.applyMods(
      "forge",
      "h_mc",
      {
        add: [{ deposit: "D2", sha256: "a2", replace: "lithium-0.11.2.jar" }],
        retire: [],
        restore: [],
        disable: [],
        enable: ["carpet.jar.disabled"],
      },
      PASSWORD,
    );
    if (outcome.kind !== "accepted") throw new Error("accepté attendu");
    sim.mods.toStep("forge", outcome.operationId, "place");
    sim.mods.succeed("forge", outcome.operationId);
    const after = await sim.readMods("forge", "h_mc");
    if (after.kind !== "ready") throw new Error("liste attendue");
    const byFile = Object.fromEntries(after.view.mods.map((mod) => [mod.file, mod]));
    expect(byFile["lithium-0.11.2.jar"]?.place).toBe("retired");
    expect(byFile["lithium-0.11.4.jar"]).toMatchObject({ place: "active", origin: "hearth" });
    expect(byFile["carpet.jar"]).toMatchObject({ enabled: true });
  });

  it("un échec au démarrage revient seul à l'état d'avant et dit pourquoi dans les dernières lignes", async () => {
    const sim = bridge();
    const outcome = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, retire: ["lithium-0.11.2.jar"] },
      PASSWORD,
    );
    if (outcome.kind !== "accepted") throw new Error("accepté attendu");
    sim.mods.rollBack("forge", outcome.operationId, {
      lines: ["Mod waystones requires balm 7.x"],
      reason: "missing_dependency",
      sentence: "Le mod Waystones a besoin du mod Balm, qui n'est pas installé.",
    });
    const tail = await sim.readOperationTail("forge", outcome.operationId);
    expect(tail).toMatchObject({ kind: "ready", tail: { reason: "missing_dependency" } });
    const list = await sim.readMods("forge", "h_mc");
    if (list.kind !== "ready") throw new Error("liste attendue");
    expect(list.view.mods.find((mod) => mod.file === "lithium-0.11.2.jar")?.place).toBe("active");
  });

  it("l'annulation est possible jusqu'au début de la mise en place seulement", async () => {
    const sim = bridge();
    const outcome = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, retire: ["lithium-0.11.2.jar"] },
      PASSWORD,
    );
    if (outcome.kind !== "accepted") throw new Error("accepté attendu");
    sim.mods.toStep("forge", outcome.operationId, "copy");
    expect(sim.mods.cancel("forge", outcome.operationId).kind).toBe("done");
    const late = await sim.applyMods(
      "forge",
      "h_mc",
      { ...EMPTY, disable: ["lithium-0.11.2.jar"] },
      PASSWORD,
    );
    if (late.kind !== "accepted") throw new Error("accepté attendu");
    sim.mods.toStep("forge", late.operationId, "place");
    expect(sim.mods.cancel("forge", late.operationId)).toMatchObject({
      kind: "refused",
      refusal: { kind: "not_cancellable" },
    });
  });
});
