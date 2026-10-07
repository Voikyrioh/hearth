import { describe, expect, it } from "vitest";
import { LinkCommandError, type SecurityState } from "@/link";
import { SimulatedSecurity } from "./simulated-security";

const now = () => Date.parse("2026-10-07T12:00:00Z");
const OWN = "Correct-Horse-9";

function failure(run: () => unknown): string | null {
  try {
    run();
  } catch (error) {
    return error instanceof LinkCommandError ? error.failure.kind : "autre";
  }
  return null;
}

describe("the simulated security of an agent", () => {
  it("starts quiet and proven, and an agent without the feature reads as unsupported", () => {
    const sim = new SimulatedSecurity(now);
    const read = sim.read("forge");
    expect(read.kind).toBe("known");
    if (read.kind === "known") {
      expect(read.state.alert.own).toBe(false);
      expect(read.state.attackMode.state).toBe("off");
      expect(read.state.device).toBe("proven");
      expect(read.state.keyAtHand).toBe(true);
    }
    sim.setSupported("old", false);
    expect(sim.read("old")).toEqual({ kind: "unsupported" });
  });

  it("publishes each change with a growing sequence and replays the last state to a late listener", () => {
    const sim = new SimulatedSecurity(now);
    const seen: SecurityState[] = [];
    sim.subscribe((state) => seen.push(state));
    sim.setAlert("forge", { own: true, others: 1 });
    sim.setMode("forge", "active");
    expect(seen.map((state) => state.seq)).toEqual([1, 2]);
    // Le message du flux ne dit rien du poste.
    expect(seen.every((state) => state.device === "unknown")).toBe(true);
    const late: SecurityState[] = [];
    sim.subscribe((state) => late.push(state));
    expect(late).toHaveLength(1);
    expect(late[0]).toMatchObject({
      seq: 2,
      alert: { own: true, others: 1 },
      attackMode: { state: "active" },
    });
  });

  it("gives the date of an alert and the countdown of a suspended mode", () => {
    const sim = new SimulatedSecurity(now);
    sim.setAlert("forge", { own: true });
    sim.setMode("forge", "suspended", { resumesInS: 600 });
    const state = sim.current("forge");
    expect(state.alert.since).toBe("2026-10-07T12:00:00.000Z");
    expect(state.attackMode.resumesInS).toBe(600);
    sim.setMode("forge", "off", { lastEnd: "auto" });
    expect(sim.current("forge").attackMode).toMatchObject({
      state: "off",
      lastEnd: "auto",
      resumesInS: null,
    });
  });

  it("applies the agent's refusals in the agent's order", () => {
    const sim = new SimulatedSecurity(now);
    // Pas de clé au coffre : rien ne part, même pour un Lecture seule.
    sim.setDevice("forge", "none", false);
    expect(failure(() => sim.change("forge", true, OWN, OWN, "admin"))).toBe("not_recognized");
    expect(failure(() => sim.change("forge", true, OWN, OWN, "readonly"))).toBe("not_recognized");
    // Clé là, mais poste non prouvé : l'agent refuse.
    sim.setDevice("forge", "none", true);
    expect(failure(() => sim.change("forge", true, OWN, OWN, "admin"))).toBe("not_recognized");
    sim.setDevice("forge", "proven");
    expect(failure(() => sim.change("forge", true, OWN, OWN, "readonly"))).toBe("forbidden");
    expect(sim.change("forge", true, "faux", OWN, "admin")).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    expect(sim.current("forge").attackMode.state).toBe("off");
    sim.setSupported("old", false);
    expect(sim.change("old", true, OWN, OWN, "admin")).toEqual({
      kind: "refused",
      refusal: { kind: "unsupported" },
    });
  });

  it("activates, deactivates and stays idempotent", () => {
    const sim = new SimulatedSecurity(now);
    const seen: SecurityState[] = [];
    sim.subscribe((state) => seen.push(state));
    const on = sim.change("forge", true, OWN, OWN, "admin");
    expect(on).toMatchObject({ kind: "done", attackMode: { state: "active" } });
    expect(seen).toHaveLength(1);
    sim.change("forge", true, OWN, OWN, "admin");
    expect(seen).toHaveLength(1);
    const off = sim.change("forge", false, OWN, OWN, "admin");
    expect(off).toMatchObject({ kind: "done", attackMode: { state: "off", lastEnd: "manual" } });
    expect(seen).toHaveLength(2);
  });
});
