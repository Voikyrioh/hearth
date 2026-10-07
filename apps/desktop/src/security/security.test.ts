import { describe, expect, it } from "vitest";
import type { SecurityState } from "@/link";
import type { ServerSecurity } from "@/stores/security";
import { attackModeBlock, blockMessageKey } from "./gate";
import { markOf, resumeMinutes } from "./mark";

function state(patch: Partial<SecurityState> = {}): SecurityState {
  return {
    serverId: "s1",
    seq: 1,
    alert: { own: false, since: null, others: null },
    attackMode: { state: "off", since: null, resumesInS: null, lastEnd: null },
    device: "proven",
    keyAtHand: true,
    ...patch,
  };
}

function entry(patch: Partial<ServerSecurity> = {}): ServerSecurity {
  return { status: "ready", state: state(), at: 0, ...patch };
}

describe("why the attack mode cannot be changed from here", () => {
  it("lets a proven administrator with a key act", () => {
    expect(attackModeBlock("admin", entry())).toBeNull();
  });

  it("puts read-only first, then the old agent, then the PC without a key", () => {
    // Lecture seule prime même sur un agent ancien et un poste sans clé.
    expect(attackModeBlock("readonly", entry({ status: "unsupported", state: null }))).toBe(
      "readonly",
    );
    expect(attackModeBlock("admin", entry({ status: "unsupported", state: null }))).toBe(
      "agent_old",
    );
    // Q18 : la preuve de toute clé inscrite du compte est acceptée, la session n'a pas à être « prouvée ».
    expect(attackModeBlock("admin", entry({ state: state({ device: "none" }) }))).toBeNull();
    expect(attackModeBlock("admin", entry({ state: state({ keyAtHand: false }) }))).toBe(
      "not_enrolled",
    );
  });

  it("says nothing while the state is not read, never a wrong reason", () => {
    expect(attackModeBlock("admin", undefined)).toBe("pending");
    expect(attackModeBlock("admin", entry({ status: "loading", state: null }))).toBe("pending");
    expect(attackModeBlock("admin", entry({ state: state({ device: "unknown" }) }))).toBe(
      "pending",
    );
    // Lecture ratée : on le dit (« Réessayer »), jamais un bouton muet ni un faux « non enregistré ».
    expect(attackModeBlock("admin", entry({ status: "error", state: null }))).toBe("unreadable");
    expect(
      attackModeBlock("admin", entry({ status: "error", state: state({ device: "unknown" }) })),
    ).toBe("unreadable");
    // Lecture ratée mais poste déjà connu : le geste reste possible, l'agent juge.
    expect(attackModeBlock("admin", entry({ status: "error" }))).toBeNull();
    expect(blockMessageKey("unreadable")).toBe("security.loadFailed");
    expect(blockMessageKey("pending")).toBeUndefined();
    expect(blockMessageKey(null)).toBeUndefined();
  });

  it("has one text per reason", () => {
    expect(blockMessageKey("readonly")).toBe("security.noPermission");
    expect(blockMessageKey("agent_old")).toBe("security.agentTooOld");
    expect(blockMessageKey("not_enrolled")).toBe("security.notEnrolled");
  });
});

describe("the mark of a server", () => {
  it("orders the attack mode, then suspended, then the alert, then nothing", () => {
    const alert = { own: true, since: null, others: null };
    const mode = (s: "off" | "active" | "suspended") => ({
      state: s,
      since: null,
      resumesInS: null,
      lastEnd: null,
    });
    expect(markOf(null)).toBeNull();
    expect(markOf(state())).toBeNull();
    expect(markOf(state({ alert }))).toBe("alert");
    expect(markOf(state({ alert: { own: false, since: null, others: 2 } }))).toBe("alert");
    expect(markOf(state({ alert, attackMode: mode("suspended") }))).toBe("suspended");
    expect(markOf(state({ alert, attackMode: mode("active") }))).toBe("attack");
  });
});

describe("the minutes before the mode resumes", () => {
  const suspended = state({
    attackMode: { state: "suspended", since: null, resumesInS: 1200, lastEnd: null },
  });

  it("counts from when the state was read, rounded up to the minute", () => {
    expect(resumeMinutes(suspended, 0, 0)).toBe(20);
    expect(resumeMinutes(suspended, 0, 30_000)).toBe(20);
    expect(resumeMinutes(suspended, 0, 61_000)).toBe(19);
    expect(resumeMinutes(suspended, 0, 1_141_000)).toBe(0);
    expect(resumeMinutes(suspended, 0, 5_000_000)).toBe(0);
  });

  it("is null when the mode is not suspended", () => {
    expect(resumeMinutes(state(), 0, 0)).toBeNull();
    expect(resumeMinutes(null, null, 0)).toBeNull();
  });
});
