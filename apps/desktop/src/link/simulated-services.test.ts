import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { ServicesEvent as Wire } from "@/bindings";
import { type ServicesEvent, SimulatedLinkBridge } from "@/link";
import { toServicesEvent } from "./services";

// Le pont simulé doit produire les MÊMES formes que le vrai protocole, sinon les tests de l'interface ne
// prouvent rien. Trois garde-fous :
// 1. les types : le simulateur construit des `ServicesEvent` générés (`bindings.ts`), `tsc` refuse tout
//    champ en trop, manquant ou mal typé ;
// 2. la fiche de référence `src-tauri/tests/fixtures/services-event.json`, que la COQUILLE compare à
//    `hearth_proto::api::services` champ par champ et valeur par valeur (tests/services_runtime.rs) : ici, le
//    simulateur doit porter exactement les mêmes chemins de champs ;
// 3. rien qui ressemble à une variable d'environnement ou à une commande de lancement.
const fixture = JSON.parse(
  // Vitest tourne depuis le dossier du client (`apps/desktop`).
  readFileSync(
    join(process.cwd(), "src-tauri", "tests", "fixtures", "services-event.json"),
    "utf8",
  ),
) as { supported: Wire; unsupported: Wire };

/** Tous les chemins de champs d'une valeur (un tableau est vu par l'union de ses éléments, `null` compte comme un chemin). */
function paths(value: unknown, prefix = ""): Set<string> {
  const out = new Set<string>();
  if (Array.isArray(value)) {
    for (const item of value) for (const path of paths(item, `${prefix}[]`)) out.add(path);
    if (value.length === 0) out.add(`${prefix}[]`);
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

function sorted(set: Set<string>): string[] {
  // Un tableau vide n'apporte pas de chemin d'élément : on compare les chemins des champs, pas les marqueurs de tableau vide.
  return [...set].filter((path) => !path.endsWith("[]")).sort();
}

function bridge() {
  return new SimulatedLinkBridge({ now: () => Date.parse("2026-10-13T12:00:00Z") });
}

describe("les formes du pont simulé sont celles du protocole", () => {
  it("un état complet porte exactement les champs de la fiche de référence", () => {
    const sim = bridge();
    sim.services.seed(
      "forge",
      [
        {
          key: "c_9f0a",
          name: "web",
          driver: "supervisor",
          unit: "docker-web.service",
          composeProject: "shop",
          state: "restart_loop",
          exitCode: 137,
          restarts10m: 4,
          removedOnStop: true,
          ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" }],
          mounts: [{ kind: "folder", source: "/srv/data", target: "/data", readOnly: true }],
          envNames: ["A"],
          command: ["nginx"],
          adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
          busy: { kind: "update", by: "marie", since: "2026-10-13T09:30:00Z" },
        },
      ],
      { name: "Docker Engine - Community" },
    );
    const event = sim.services.current("forge");
    expect(event).toBeDefined();
    expect(sorted(paths(event))).toEqual(sorted(paths(fixture.supported)));
  });

  it("un service sans option porte les mêmes champs, les absents valent null", () => {
    const sim = bridge();
    sim.services.seed("forge", [{ key: "c_1", name: "a", image: { ref: null, tag: null } }]);
    const sim1 = sim.services.current("forge");
    const view = sim1?.view;
    expect(view?.kind).toBe("supported");
    if (view?.kind !== "supported") return;
    const [item] = view.services;
    expect(Object.keys(item ?? {}).sort()).toEqual(
      Object.keys((fixture.supported.view as { services: object[] }).services[1] ?? {}).sort(),
    );
    expect(item?.image).toEqual({ ref: null, tag: null, shortId: "0123456789ab" });
    expect(item?.driver).toBeNull();
    expect(item?.unit).toBeNull();
  });

  it("l'agent sans la capacité porte la forme de la fiche", () => {
    const sim = bridge();
    sim.services.setUnsupported("salon", "0.1.0");
    expect(sorted(paths(sim.services.current("salon")))).toEqual(
      sorted(paths(fixture.unsupported)),
    );
  });

  it("la fiche elle-même est lue par le convertisseur de l'interface", () => {
    const event = toServicesEvent(fixture.supported);
    expect(event.kind).toBe("state");
    if (event.kind !== "state") return;
    expect(event.engine.lastReadyAt).toBe(Date.parse("2026-10-13T10:00:00Z"));
    expect(event.engine.host).toEqual(
      fixture.supported.view.kind === "supported" ? fixture.supported.view.engine.host : null,
    );
    expect(event.services[0]).toMatchObject({
      key: "c_9f0a",
      state: "restart_loop",
      exitCode: 137,
      restarts10m: 4,
      since: Date.parse("2026-10-13T09:00:00Z"),
      image: { ref: "nginx", tag: "1.27", shortId: "0123456789ab" },
    });
    expect(event.services[1]).toMatchObject({ since: null, createdAt: null, exitCode: null });
    const unsupported = toServicesEvent(fixture.unsupported);
    expect(unsupported).toEqual({
      kind: "unsupported",
      serverId: "salon",
      seq: 1,
      agentVersion: "0.1.0",
    });
  });

  it("aucun champ ne désigne une VALEUR de variable : les noms et la commande vont à la liste, jamais une valeur", () => {
    const sim = bridge();
    sim.services.generate("forge", 20);
    sim.services.setEnv("forge", "c_0000", [{ name: "MOT_DE_PASSE", value: "valeur-sentinelle" }]);
    const found = [...paths(sim.services.current("forge"))].map((path) => path.toLowerCase());
    for (const word of ["env", "secret", "value", "values", "password"]) {
      expect(found.filter((path) => path.split(/[.[\]]/).includes(word))).toEqual([]);
    }
    const converted = toServicesEvent(sim.services.current("forge") as Wire);
    const all = JSON.stringify(converted);
    expect(all).toContain("MOT_DE_PASSE");
    expect(all).not.toContain("valeur-sentinelle");
    expect(JSON.stringify(sim.services.current("forge"))).not.toContain("valeur-sentinelle");
  });
});

describe("le simulateur se comporte comme l'agent", () => {
  async function listen(sim: SimulatedLinkBridge) {
    const events: ServicesEvent[] = [];
    await sim.onServices((event) => events.push(event));
    return events;
  }

  it("rejoue le dernier état à l'abonnement puis annonce chaque changement, avec un numéro croissant", async () => {
    const sim = bridge();
    sim.services.seed("forge", [{ key: "c_1", name: "b" }]);
    const events = await listen(sim);
    expect(events).toHaveLength(1);
    sim.services.upsert("forge", { key: "c_2", name: "A" });
    sim.services.upsert("forge", { key: "c_1", name: "b", state: "stopped" });
    sim.services.remove("forge", "c_2");
    const seqs = events.map((event) => event.seq);
    expect(seqs).toEqual([...seqs].sort((a, b) => a - b));
    expect(new Set(seqs).size).toBe(seqs.length);
    const last = events.at(-1);
    expect(last?.kind === "state" && last.services.map((s) => `${s.name}:${s.state}`)).toEqual([
      "b:stopped",
    ]);
  });

  it("trie sans tenir compte de la casse, et un renommage garde la clé", async () => {
    const sim = bridge();
    sim.services.seed("forge", [
      { key: "c_1", name: "zeta" },
      { key: "c_2", name: "Alpha" },
      { key: "c_3", name: "mid" },
    ]);
    sim.services.upsert("forge", { key: "c_1", name: "Beta" });
    const view = sim.services.current("forge")?.view;
    expect(view?.kind === "supported" && view.services.map((s) => `${s.key}:${s.name}`)).toEqual([
      "c_2:Alpha",
      "c_1:Beta",
      "c_3:mid",
    ]);
  });

  it("200 services et des changements rapides : un état complet à chaque lot, jamais de doublon de clé", async () => {
    const sim = bridge();
    sim.services.generate("forge", 200);
    const events = await listen(sim);
    for (let round = 0; round < 50; round += 1) {
      sim.services.upsertMany(
        "forge",
        Array.from({ length: 10 }, (_, index) => ({
          key: `c_${String(round * 3 + index).padStart(4, "0")}`,
          name: `service-${String(round * 3 + index).padStart(3, "0")}`,
          state: round % 2 === 0 ? "stopped" : "running",
        })),
      );
    }
    expect(events).toHaveLength(51);
    for (const event of events) {
      expect(event.kind).toBe("state");
      if (event.kind !== "state") continue;
      expect(event.services).toHaveLength(200);
      expect(new Set(event.services.map((s) => s.key)).size).toBe(200);
    }
  });

  it("ne dit rien tant que le lien n'est pas connecté, puis renvoie l'état à son retour", async () => {
    const sim = bridge();
    sim.services.seed("forge", [{ key: "c_1", name: "a" }]);
    const events = await listen(sim);
    sim.setState("forge", "offline");
    sim.services.upsert("forge", { key: "c_2", name: "b" });
    expect(events).toHaveLength(1);
    sim.setState("forge", "connected");
    expect(events).toHaveLength(2);
    expect(events[1]?.kind === "state" && events[1].services).toHaveLength(2);
  });

  it("un agent ancien, puis mis à jour, change ce que dit le simulateur", async () => {
    const sim = bridge();
    const events = await listen(sim);
    sim.services.setUnsupported("forge", "0.1.0");
    sim.services.seed("forge", [{ key: "c_1", name: "a" }]);
    expect(events.map((event) => event.kind)).toEqual(["unsupported", "state"]);
  });

  it("« Vérifier à nouveau » exige un lien connecté et note la demande", async () => {
    const sim = bridge();
    sim.services.seed("forge", [], { state: "stopped" });
    await expect(sim.recheckEngine("forge")).resolves.toMatchObject({ state: "stopped" });
    expect(sim.services.rechecks).toEqual(["forge"]);
    sim.setState("forge", "offline");
    await expect(sim.recheckEngine("forge")).rejects.toMatchObject({
      failure: { kind: "not_connected" },
    });
  });
});
