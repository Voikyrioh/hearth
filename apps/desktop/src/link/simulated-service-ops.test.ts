import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { ServiceOperationEvent as Wire } from "@/bindings";
import { LinkCommandError, type ServiceOperationEvent, SimulatedLinkBridge } from "@/link";

// Le pont simulé des opérations doit produire les MÊMES formes que le vrai protocole (HRT-84). La fiche
// `src-tauri/tests/fixtures/service-operation-event.json` est ce que la COQUILLE écrit vers l'interface : elle la
// compare, champ par champ et valeur par valeur, à ses propres types (tests/service_ops_runtime.rs). Ici le
// simulateur doit porter exactement les mêmes chemins de champs, puis tenir les mêmes promesses : une opération
// qui court encore au retour du lien est reconstruite SANS nouveau message, l'issue d'une opération finie pendant
// la coupure est annoncée UNE fois, et un compte ne voit que ce qu'il peut voir.
const fixture = JSON.parse(
  readFileSync(
    join(process.cwd(), "src-tauri", "tests", "fixtures", "service-operation-event.json"),
    "utf8",
  ),
) as { running: Wire; awaiting: Wire; gone: Wire };

function paths(value: unknown, prefix = ""): Set<string> {
  const out = new Set<string>();
  if (Array.isArray(value)) {
    for (const item of value) for (const path of paths(item, `${prefix}[]`)) out.add(path);
    return out;
  }
  if (value !== null && typeof value === "object") {
    for (const [key, inner] of Object.entries(value)) {
      const path = prefix ? `${prefix}.${key}` : key;
      out.add(path);
      for (const nested of paths(inner, path)) out.add(nested);
    }
  }
  return out;
}

const NOW = Date.parse("2026-10-14T10:00:00Z");

function bridge() {
  return new SimulatedLinkBridge({ now: () => NOW });
}

function collect(sim: SimulatedLinkBridge) {
  const events: ServiceOperationEvent[] = [];
  const done = sim.onServiceOperations((event) => events.push(event));
  return { events, done };
}

function operationOf(wire: Wire) {
  if (wire.change.kind !== "upsert") throw new Error("fiche");
  return wire.change.operation;
}

describe("les formes du pont simulé", () => {
  it("une opération en cours porte les mêmes champs que la fiche de la coquille", () => {
    const sim = bridge();
    sim.serviceOps.accept("forge", {
      key: "h_1",
      kind: "deploy",
      step: "pull",
      steps: [
        { step: "check", state: "done" },
        { step: "pull", state: "intent" },
      ],
      percent: 40,
      itemsDone: 3,
      bytesDone: 400,
      bytesTotal: 1000,
      etaS: 12,
    });
    const held = sim.serviceOps.of("forge")[0];
    expect([...paths(held)].sort()).toEqual([...paths(operationOf(fixture.running))].sort());
  });

  it("une opération qui attend une décision porte la décision de la fiche", () => {
    const sim = bridge();
    const op = sim.serviceOps.accept("forge", { key: "h_1", kind: "update" });
    sim.serviceOps.askDecision("forge", op.id, ["revert", "keep"], NOW + 900_000);
    sim.serviceOps.update("forge", op.id, { cause: "decided_by_account" });
    const held = sim.serviceOps.of("forge")[0];
    expect([...paths(held)].sort()).toEqual([...paths(operationOf(fixture.awaiting))].sort());
  });
});

describe("le retour du lien", () => {
  it("une opération qui court encore est reconstruite sans nouveau message", async () => {
    const sim = bridge();
    const op = sim.serviceOps.accept("forge", { key: "h_1", percent: 40 });
    const { events, done } = collect(sim);
    await done;
    expect(events).toHaveLength(1);
    sim.publish("forge", "reconnecting");
    sim.publish("forge", "connected");
    expect(events).toHaveLength(1);
    expect(sim.serviceOps.of("forge").map((held) => held.id)).toEqual([op.id]);
  });

  it("l'issue d'une opération finie pendant la coupure est annoncée une seule fois", async () => {
    const sim = bridge();
    const op = sim.serviceOps.accept("forge", { key: "h_1" });
    const { events, done } = collect(sim);
    await done;
    sim.publish("forge", "reconnecting");
    sim.serviceOps.finish("forge", op.id, "succeeded");
    // Rien pendant la coupure.
    expect(events).toHaveLength(1);
    sim.publish("forge", "connected");
    sim.publish("forge", "reconnecting");
    sim.publish("forge", "connected");
    const finished = events.filter(
      (event) => event.kind === "upsert" && event.operation.status === "succeeded",
    );
    expect(finished).toHaveLength(1);
  });

  it("une issue reste rejouée à l'abonnement jusqu'à son acquittement", async () => {
    const sim = bridge();
    const op = sim.serviceOps.accept("forge", { key: "h_1" });
    sim.serviceOps.finish("forge", op.id, "failed");
    const before = collect(sim);
    await before.done;
    expect(before.events.map((event) => event.kind === "upsert" && event.operation.status)).toEqual(
      ["failed"],
    );
    await sim.ackServiceOperations("forge", [op.id]);
    const after = collect(sim);
    await after.done;
    expect(after.events).toEqual([]);
  });
});

describe("qui voit quoi (BR-OPS-001), pour les quatre profils", () => {
  async function seen(setup: (sim: SimulatedLinkBridge) => void, server: "forge" | "salon") {
    const sim = bridge();
    setup(sim);
    sim.serviceOps.accept(server, { id: "A", key: "h_1", kind: "deploy" });
    sim.serviceOps.accept(server, { id: "B", key: "c_1", kind: "update" });
    const { events, done } = collect(sim);
    await done;
    return events.flatMap((event) => (event.kind === "upsert" ? [event.operation.id] : []));
  }

  it("A, administrateur avec le droit : voit les deux", async () => {
    expect(await seen(() => {}, "forge")).toEqual(["A", "B"]);
  });

  it("B, administrateur sans le droit : ne voit que le service créé par Hearth", async () => {
    expect(
      await seen((sim) => sim.setServiceRightOnServer("forge", "marie", false), "forge"),
    ).toEqual(["A"]);
  });

  it("C, lecture seule sans le droit : ne voit que le service créé par Hearth", async () => {
    expect(
      await seen((sim) => sim.setServiceRightOnServer("salon", "paul", false), "salon"),
    ).toEqual(["A"]);
  });

  it("D, lecture seule avec le droit : voit les deux", async () => {
    expect(await seen(() => {}, "salon")).toEqual(["A", "B"]);
  });
});

describe("répondre à une opération", () => {
  function waiting(sim: SimulatedLinkBridge) {
    const op = sim.serviceOps.accept("forge", { key: "h_1", kind: "update" });
    sim.serviceOps.askDecision("forge", op.id, ["revert", "keep"], NOW + 900_000);
    return op;
  }

  it("sans le droit, rien n'est décidé : échec typé, aucune décision notée", async () => {
    const sim = bridge();
    sim.setServiceRightOnServer("forge", "marie", false);
    const op = waiting(sim);
    await expect(
      sim.decideServiceOperation("forge", op.id, "keep", "Correct-Horse-9"),
    ).rejects.toMatchObject({ failure: { kind: "missing_services_right" } });
    expect(sim.serviceOps.decisions).toEqual([]);
  });

  it("répondre est un acte de pilotage : rien à saisir en état normal, le mot de passe en mode attaque (HRT-194)", async () => {
    const normal = bridge();
    const accepted = waiting(normal);
    expect(await normal.decideServiceOperation("forge", accepted.id, "keep", null)).toMatchObject({
      kind: "done",
    });
    const sim = bridge();
    const op = waiting(sim);
    sim.reauth.setTrust("forge", "attack");
    // La ceinture de la liaison : rien ne part sans mot de passe en mode attaque.
    await expect(sim.decideServiceOperation("forge", op.id, "keep", null)).rejects.toMatchObject({
      failure: { kind: "invalid_input", field: "credentials" },
    });
    expect(await sim.decideServiceOperation("forge", op.id, "keep", "faux")).toMatchObject({
      refusal: { kind: "wrong_password" },
    });
    expect(sim.serviceOps.decisions).toEqual([]);
  });

  it("un choix qui n'est pas offert est refusé avec sa raison", async () => {
    const sim = bridge();
    const op = waiting(sim);
    expect(await sim.decideServiceOperation("forge", op.id, "cancel", "Correct-Horse-9")).toEqual({
      kind: "refused",
      refusal: { kind: "not_offered" },
    });
  });

  it("« Revenir » démarre un retour arrière, jamais un « revenu » sans geste (BR-EDIT-031)", async () => {
    const sim = bridge();
    const op = waiting(sim);
    const outcome = await sim.decideServiceOperation("forge", op.id, "revert", "Correct-Horse-9");
    expect(outcome).toMatchObject({ kind: "done", operation: { status: "rolling_back" } });
  });

  it("une décision expirée est refusée avant tout", async () => {
    const sim = bridge();
    const op = sim.serviceOps.accept("forge", { key: "h_1", kind: "update" });
    sim.serviceOps.askDecision("forge", op.id, ["keep"], NOW - 1);
    expect(await sim.decideServiceOperation("forge", op.id, "keep", "Correct-Horse-9")).toEqual({
      kind: "refused",
      refusal: { kind: "expired" },
    });
  });

  it("hors « Connecté », rien n'est envoyé", async () => {
    const sim = bridge();
    const op = waiting(sim);
    sim.publish("forge", "offline");
    await expect(
      sim.decideServiceOperation("forge", op.id, "keep", "Correct-Horse-9"),
    ).rejects.toBeInstanceOf(LinkCommandError);
  });
});
