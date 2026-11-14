import { describe, expect, it } from "vitest";
import type { DeleteRequest } from "./serviceDelete";
import { SimulatedLinkBridge } from "./simulated";
import { LinkCommandError } from "./types";

// L'agent simulé de la suppression juge COMME LE VRAI (`crates/hearth-agent/src/application/delete.rs`) : un test qui
// passe ici passerait contre l'agent. La confirmation (jamais couverte par l'élévation) et le droit sont jugés avant.
const PASSWORD = "Correct-Horse-9";
const KEEP: DeleteRequest = { eraseData: false, eraseBackups: false, confirmName: null };

function bridge() {
  const sim = new SimulatedLinkBridge();
  sim.services.seed("forge", [
    {
      key: "h_web",
      name: "web",
      origin: "hearth",
      graceS: 30,
      mounts: [
        { kind: "folder", source: "/srv/hearth/data/web", target: "/data", readOnly: false },
      ],
    },
    { key: "h_db", name: "base", origin: "hearth", state: "stopped" },
    { key: "h_pause", name: "pause", origin: "hearth", state: "paused" },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
    { key: "e_free", name: "libre", origin: "external" },
  ]);
  return sim;
}

describe("la consultation de la fenêtre", () => {
  it("rend le nom, l'image, l'arrêt annoncé et un dossier géré par défaut", async () => {
    const outcome = await bridge().readDeletePreview("forge", "h_web");
    expect(outcome.kind).toBe("ready");
    if (outcome.kind !== "ready") return;
    expect(outcome.preview).toMatchObject({
      name: "web",
      image: "nginx:1.27",
      running: true,
      stopGraceS: 30,
      containerMissing: false,
      backups: 0,
    });
    expect(outcome.preview.folders[0]).toMatchObject({
      path: "/srv/hearth/data/web",
      fate: "erasable",
      partial: false,
    });
  });

  it("un service sans dossier monté n'a aucun dossier géré : le détail et la fenêtre disent la même chose", async () => {
    const outcome = await bridge().readDeletePreview("forge", "h_pause");
    expect(outcome).toMatchObject({ kind: "ready", preview: { folders: [], serverFolders: [] } });
  });

  it("un service arrêté ne s'arrêtera pas", async () => {
    const outcome = await bridge().readDeletePreview("forge", "h_db");
    expect(outcome).toMatchObject({ kind: "ready", preview: { running: false } });
  });

  it("un service adopté est refusé par son code, un conteneur jamais confié comme un service inconnu", async () => {
    const sim = bridge();
    expect(await sim.readDeletePreview("forge", "e_old")).toEqual({
      kind: "refused",
      refusal: { kind: "service_adopted" },
    });
    expect(await sim.readDeletePreview("forge", "e_free")).toEqual({
      kind: "refused",
      refusal: { kind: "service_not_found" },
    });
    expect(await sim.readDeletePreview("forge", "h_inconnu")).toEqual({
      kind: "refused",
      refusal: { kind: "service_not_found" },
    });
  });

  it("un moteur pas prêt est refusé avec son état", async () => {
    const sim = bridge();
    sim.services.setEngine("forge", { state: "stopped" });
    expect(await sim.readDeletePreview("forge", "h_web")).toEqual({
      kind: "refused",
      refusal: { kind: "engine_unavailable", engine: "stopped" },
    });
  });

  it("sans le droit de gérer les services : échec typé, jamais une prévisualisation", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("forge", "marie", false);
    await expect(sim.readDeletePreview("forge", "h_web")).rejects.toBeInstanceOf(LinkCommandError);
  });

  it("lien coupé : échec typé, rien n'est lu", async () => {
    const sim = bridge();
    sim.publish("forge", "reconnecting");
    await expect(sim.readDeletePreview("forge", "h_web")).rejects.toMatchObject({
      failure: { kind: "not_connected" },
    });
  });
});

describe("la demande de suppression", () => {
  it("accepte une demande sans case et lance une opération « delete » acceptée, annulable", async () => {
    const sim = bridge();
    const outcome = await sim.deleteService("forge", "h_web", KEEP, PASSWORD);
    expect(outcome.kind).toBe("accepted");
    const [running] = sim.deletes.running("forge");
    expect(running?.key).toBe("h_web");
    const operation = sim.serviceOps.of("forge")[0];
    expect(operation).toMatchObject({ kind: "delete", status: "running", cancellable: true });
    // Le nom est réservé pendant l'opération (BR-DEL-019).
    expect(sim.services.find("forge", "h_web")?.busy).not.toBeNull();
  });

  it("redemande le mot de passe même sous l'élévation de cinq minutes (BR-DEL-012)", async () => {
    const sim = bridge();
    sim.reauth.open("forge");
    const outcome = await sim.deleteService("forge", "h_web", KEEP, null);
    expect(outcome).toMatchObject({ kind: "refused", refusal: { kind: "password_required" } });
    expect(sim.deletes.received).toEqual([]);
  });

  it("refuse un mauvais mot de passe sans rien lancer", async () => {
    const sim = bridge();
    const outcome = await sim.deleteService("forge", "h_web", KEEP, "faux");
    expect(outcome).toMatchObject({ kind: "refused", refusal: { kind: "wrong_password" } });
    expect(sim.deletes.received).toEqual([]);
  });

  it("refuse dans l'ordre de l'agent : adopté, occupé, état, sauvegardes, nom retapé", async () => {
    const sim = bridge();
    const send = (key: string, request = KEEP) =>
      sim.deleteService("forge", key, request, PASSWORD);
    expect(await send("e_old")).toMatchObject({ refusal: { kind: "service_adopted" } });
    expect(await send("e_free")).toMatchObject({ refusal: { kind: "service_not_found" } });
    expect(await send("h_pause")).toMatchObject({
      refusal: { kind: "not_drivable", state: "paused" },
    });
    expect(
      await send("h_db", { eraseData: false, eraseBackups: true, confirmName: "base" }),
    ).toMatchObject({
      refusal: { kind: "backups_unavailable" },
    });
    expect(
      await send("h_db", { eraseData: true, eraseBackups: false, confirmName: "Base" }),
    ).toMatchObject({
      refusal: { kind: "name_mismatch" },
    });
    expect(
      await send("h_db", { eraseData: true, eraseBackups: false, confirmName: null }),
    ).toMatchObject({
      refusal: { kind: "name_mismatch" },
    });
    sim.serviceOps.accept("forge", { key: "h_web", kind: "backup" }, "claire");
    expect(await send("h_web")).toMatchObject({
      refusal: { kind: "service_busy", by: "claire", action: "backup" },
    });
    expect(sim.deletes.received).toEqual([]);
  });

  it("la clôture : succès retire le service et libère le nom, l'échec avant le point de non-retour le laisse entier", async () => {
    const sim = bridge();
    const first = await sim.deleteService("forge", "h_web", KEEP, PASSWORD);
    if (first.kind !== "accepted") throw new Error("accepté attendu");
    sim.deletes.toStep("forge", first.operationId, "remove_container");
    expect(sim.services.find("forge", "h_web")).toBeUndefined();
    sim.deletes.succeed("forge", first.operationId);
    expect(sim.serviceOps.of("forge").find((op) => op.id === first.operationId)).toMatchObject({
      status: "succeeded",
    });
    const second = await sim.deleteService("forge", "h_db", KEEP, PASSWORD);
    if (second.kind !== "accepted") throw new Error("accepté attendu");
    sim.deletes.fail("forge", second.operationId, "engine_refused");
    expect(sim.services.find("forge", "h_db")).toBeDefined();
    expect(sim.serviceOps.of("forge").find((op) => op.id === second.operationId)).toMatchObject({
      status: "failed",
      cause: "engine_refused",
    });
  });

  it("annuler avant le point de non-retour clôt « cancelled » ; après, « too_late »", async () => {
    const sim = bridge();
    const first = await sim.deleteService("forge", "h_web", KEEP, PASSWORD);
    const second = await sim.deleteService("forge", "h_db", KEEP, PASSWORD);
    if (first.kind !== "accepted" || second.kind !== "accepted") throw new Error("accepté attendu");
    sim.deletes.toStep("forge", second.operationId, "remove_container");
    expect(await sim.cancelServiceOperation("forge", second.operationId, PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "too_late" },
    });
    const cancelled = await sim.cancelServiceOperation("forge", first.operationId, PASSWORD);
    expect(cancelled).toMatchObject({ kind: "done", operation: { status: "cancelling" } });
    await new Promise((resolve) => setTimeout(resolve, 5));
    expect(sim.serviceOps.of("forge").find((op) => op.id === first.operationId)).toMatchObject({
      status: "cancelled",
      cause: "cancelled_by_account",
    });
    expect(sim.services.find("forge", "h_web")).toBeDefined();
  });

  it("un agent d'avant ne montre pas la capacité de suppression", async () => {
    const sim = bridge();
    expect((await sim.readServiceRights("forge")).serviceDelete).toBe(true);
    sim.accounts.setServiceDeleteServed("forge", false);
    expect((await sim.readServiceRights("forge")).serviceDelete).toBe(false);
  });
});
