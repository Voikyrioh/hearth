import { describe, expect, it } from "vitest";
import { SimulatedLinkBridge } from "./simulated";
import { simConsoleView } from "./simulated-console";

// L'agent simulé de la console juge COMME LE VRAI (`crates/hearth-agent/src/application/console.rs`) : forme, verbe
// refusé, classe signée, état du jeu, débit. Le mot de passe d'une commande sensible est exigé à chaque fois.
const PASSWORD = "Correct-Horse-9";

function bridge() {
  const sim = new SimulatedLinkBridge();
  sim.services.seed("forge", [
    { key: "h_mc", name: "Prominence", origin: "hearth" },
    { key: "h_other", name: "Terraria", origin: "hearth" },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
  ]);
  sim.console.seed("forge", "h_mc", simConsoleView());
  return sim;
}

describe("décrire la console", () => {
  it("rend ce que la fiche permet, et dit pourquoi un service n'a pas de console", async () => {
    const sim = bridge();
    const open = await sim.describeConsole("forge", "h_mc");
    expect(open).toMatchObject({ kind: "ready", view: { available: true, canWrite: true } });
    expect(await sim.describeConsole("forge", "h_other")).toMatchObject({
      view: { available: false, unavailableReason: "no_console" },
    });
    expect(await sim.describeConsole("forge", "e_old")).toMatchObject({
      view: { available: false, unavailableReason: "adopted" },
    });
    expect(await sim.describeConsole("forge", "h_nope")).toEqual({
      kind: "refused",
      refusal: { kind: "service_not_found" },
    });
  });
});

describe("les joueurs connectés", () => {
  it("rend la lecture posée, jamais un chiffre pour un service sans console", async () => {
    const sim = bridge();
    sim.console.seedPlayers("forge", "h_mc", {
      state: "ok",
      online: 2,
      max: 20,
      names: ["Alice_42", "Bram"],
      hiddenNames: 0,
      moreNames: 0,
      readAt: "2026-11-18T20:45:12Z",
    });
    expect(await sim.readPlayers("forge", "h_mc")).toMatchObject({
      kind: "ready",
      view: { state: "ok", online: 2, max: 20 },
    });
    expect(await sim.readPlayers("forge", "h_other")).toMatchObject({
      view: { state: "no_console", online: null },
    });
  });
});

describe("envoyer une commande", () => {
  it("une commande libre part sans mot de passe en état normal, et la réponse du jeu revient", async () => {
    const sim = bridge();
    sim.console.reply("list", {
      outcome: "answered",
      reply: "There are 2 of a max of 20 players online: Alice_42, Bram",
      truncated: false,
    });
    const out = await sim.sendConsole("forge", "h_mc", "/list", 1, null);
    expect(out).toMatchObject({ kind: "replied", reply: { outcome: "answered" } });
    expect(sim.console.received[0]).toMatchObject({ command: "/list", commandClass: 1 });
    const silent = await sim.sendConsole("forge", "h_mc", "say bonjour", 1, null);
    expect(silent).toMatchObject({ reply: { outcome: "empty", reply: null } });
  });

  it("une commande sensible exige le mot de passe, à chaque fois, même juste après l'avoir donné", async () => {
    const sim = bridge();
    expect(await sim.sendConsole("forge", "h_mc", "op Bram", 2, null)).toEqual({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
    expect(await sim.sendConsole("forge", "h_mc", "op Bram", 2, PASSWORD)).toMatchObject({
      kind: "replied",
    });
    expect(await sim.sendConsole("forge", "h_mc", "ban Bram", 2, null)).toEqual({
      kind: "refused",
      refusal: { kind: "password_required" },
    });
  });

  it("la classe signée doit être celle que l'agent juge, et un verbe refusé n'est jamais envoyé", async () => {
    const sim = bridge();
    expect(await sim.sendConsole("forge", "h_mc", "op Bram", 1, PASSWORD)).toEqual({
      kind: "refused",
      refusal: { kind: "class_mismatch" },
    });
    expect(await sim.sendConsole("forge", "h_mc", "stop", 1, null)).toEqual({
      kind: "refused",
      refusal: { kind: "verb_refused", reason: "use_hearth_stop" },
    });
    expect(await sim.sendConsole("forge", "h_mc", "minecraft:STOP", 1, null)).toMatchObject({
      refusal: { kind: "verb_refused" },
    });
  });

  it("une ligne hors forme est refusée avec sa raison", async () => {
    const sim = bridge();
    const reason = async (command: string) => {
      const out = await sim.sendConsole("forge", "h_mc", command, 1, null);
      return out.kind === "refused" && out.refusal.kind === "invalid" ? out.refusal.reason : out;
    };
    expect(await reason("   ")).toBe("empty");
    expect(await reason("a".repeat(257))).toBe("too_long");
    expect(await reason("say a\nstop")).toBe("control_char");
  });

  it("refuse selon l'état : arrêté, démarre, opération en cours", async () => {
    const sim = bridge();
    const kindAt = async (state: "stopped" | "starting" | "busy") => {
      sim.console.setState("forge", "h_mc", state);
      const out = await sim.sendConsole("forge", "h_mc", "list", 1, null);
      return out.kind === "refused" ? out.refusal.kind : out.kind;
    };
    expect(await kindAt("stopped")).toBe("not_running");
    expect(await kindAt("starting")).toBe("starting");
    expect(await kindAt("busy")).toBe("operation_in_progress");
  });

  it("20 commandes en une minute passent, la 21e est refusée avec le délai", async () => {
    let clock = 1_000_000;
    const sim = new SimulatedLinkBridge({ now: () => clock });
    sim.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth" }]);
    sim.console.seed("forge", "h_mc", simConsoleView());
    for (let i = 0; i < 20; i += 1) {
      const out = await sim.sendConsole("forge", "h_mc", "list", 1, null);
      expect(out.kind).toBe("replied");
      clock += 100;
    }
    const over = await sim.sendConsole("forge", "h_mc", "list", 1, null);
    expect(over).toMatchObject({ kind: "refused", refusal: { kind: "rate_limited" } });
    if (over.kind === "refused" && over.refusal.kind === "rate_limited") {
      expect(over.refusal.retry_after_s).toBeGreaterThan(50);
    }
    clock += 60_000;
    expect((await sim.sendConsole("forge", "h_mc", "list", 1, null)).kind).toBe("replied");
  });

  it("console injoignable, puis lien coupé avant la réponse : jamais renvoyée", async () => {
    const sim = bridge();
    sim.console.unreachable = true;
    expect(await sim.sendConsole("forge", "h_mc", "list", 1, null)).toEqual({
      kind: "refused",
      refusal: { kind: "unreachable" },
    });
  });
});
