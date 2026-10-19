import { describe, expect, it } from "vitest";
import type { ServiceInfo, ServiceState } from "@/link";
import { alreadyText, offeredGestures, refusalText, warningsOf } from "@/services/lifecycle";

// Les gestes de pilotage et leurs textes (HRT-65, BR-LIFE-003, 008, 014) : des fonctions pures.
describe("les gestes offerts par l'état (BR-LIFE-003)", () => {
  const table: [ServiceState, string[]][] = [
    ["running", ["stop", "restart"]],
    ["stopped", ["start"]],
    ["error", ["start"]],
    ["restart_loop", ["stop", "restart"]],
    ["starting", []],
    ["stopping", []],
    ["deploying", []],
    ["updating", []],
    ["paused", []],
    ["unknown", []],
  ];
  it.each(table)("%s", (state, expected) => {
    expect([...offeredGestures(state)]).toEqual(expected);
  });
});

describe("les textes", () => {
  it("un refus se dit par son code, avec le compte seulement s'il est valide", () => {
    expect(
      refusalText({ kind: "service_busy", by: "marie", action: "backup" } as never, {
        name: "web",
      }),
    ).toBe("Une action est déjà en cours sur ce service. Lancée par marie.");
    expect(
      refusalText({ kind: "service_busy", by: "<b>x</b>", action: null } as never, { name: "web" }),
    ).toBe("Une action est déjà en cours sur ce service.");
    expect(refusalText({ kind: "from_the_future" }, { name: "web" })).toBe(
      "Le serveur a refusé l'action. Le service n'a pas été modifié.",
    );
    expect(alreadyText("stopped", { name: "web" })).toBe("Le service web est déjà arrêté.");
    expect(alreadyText("running", { name: "web" })).toBe("Le service web est déjà en marche.");
  });

  it("un service géré par le système remplace la règle « Redémarre toujours »", () => {
    const service = {
      adopted: null,
      restart: "always",
      driver: "supervisor",
      unit: "docker-terraria.service",
    } as Pick<ServiceInfo, "adopted" | "restart" | "driver" | "unit">;
    expect(warningsOf(service).map((warning) => warning.id)).toEqual(["unit"]);
    expect(warningsOf({ ...service, driver: "engine", unit: null }).map((w) => w.id)).toEqual([
      "always",
    ]);
  });
});
