import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { SecurityEvent } from "@/bindings";
import type { SecurityState } from "./security";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };

function ipc(handler: (cmd: string, args: Record<string, unknown>) => unknown) {
  const calls: Call[] = [];
  mockIPC(
    (cmd, args) => {
      const payload = (args ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args: payload });
      return handler(cmd, payload);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

afterEach(() => clearMocks());

const EVENT: SecurityEvent = {
  serverId: "s1",
  seq: 4,
  alert: { own: true, since: "2026-10-07T01:00:00Z", others: 2 },
  attackMode: {
    state: "suspended",
    since: "2026-10-07T00:30:00Z",
    resumesInS: 1200,
    lastEnd: null,
  },
  device: "proven",
  keyAtHand: true,
  erasurePending: false,
};

describe("security commands of the real bridge", () => {
  it("sends ONE typed command per call, with business parameters only", async () => {
    const calls = ipc((cmd) =>
      cmd === "get_security"
        ? { kind: "known", snapshot: EVENT }
        : { kind: "done", attack_mode: EVENT.attackMode },
    );
    const bridge = new TauriLinkBridge();
    await bridge.getSecurity("s1");
    await bridge.setAttackMode("s1", true, "Mot-De-Passe-12");
    expect(calls).toEqual([
      { cmd: "get_security", args: { serverId: "s1" } },
      {
        cmd: "set_attack_mode",
        args: { serverId: "s1", active: true, password: "Mot-De-Passe-12" },
      },
    ]);
    // Aucune route, aucune méthode, aucun corps, aucune clé : jamais un paramètre libre.
    for (const call of calls) {
      expect(Object.keys(call.args)).not.toEqual(expect.arrayContaining(["path"]));
      expect(Object.keys(call.args)).not.toEqual(expect.arrayContaining(["device"]));
    }
  });

  it("reads the state, and an agent without the feature as unsupported", async () => {
    const bridge = new TauriLinkBridge();
    ipc(() => ({ kind: "known", snapshot: EVENT }));
    expect(await bridge.getSecurity("s1")).toEqual({ kind: "known", state: EVENT });
    clearMocks();
    ipc(() => ({ kind: "unsupported" }));
    expect(await bridge.getSecurity("s1")).toEqual({ kind: "unsupported" });
  });

  it("reads every outcome of a change, the unknown one with its operation key", async () => {
    const bridge = new TauriLinkBridge();
    ipc(() => ({ kind: "done", attack_mode: EVENT.attackMode }));
    expect(await bridge.setAttackMode("s1", true, "x")).toEqual({
      kind: "done",
      attackMode: EVENT.attackMode,
    });
    clearMocks();
    ipc(() => ({ kind: "refused", refusal: { kind: "wrong_password" } }));
    expect(await bridge.setAttackMode("s1", true, "x")).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    clearMocks();
    ipc(() => ({ kind: "unknown", op_id: "OP1" }));
    expect(await bridge.setAttackMode("s1", false, "x")).toEqual({ kind: "unknown", opId: "OP1" });
  });

  it("rejects with the typed failure: no key (not_recognized) and a read-only account (forbidden)", async () => {
    const bridge = new TauriLinkBridge();
    for (const kind of ["not_recognized", "forbidden", "device_challenge_unavailable"]) {
      clearMocks();
      ipc(() => {
        throw { kind };
      });
      const error = await bridge.setAttackMode("s1", true, "x").catch((e) => e);
      expect(error).toBeInstanceOf(LinkCommandError);
      expect((error as LinkCommandError).failure.kind).toBe(kind);
    }
  });
});

describe("the security event of the real bridge", () => {
  it("listens first, then replays the last known state of each server", async () => {
    const order: string[] = [];
    const seen: SecurityState[] = [];
    ipc((cmd) => {
      order.push(cmd);
      return cmd === "list_security_states" ? [EVENT] : null;
    });
    const bridge = new TauriLinkBridge();
    const off = await bridge.onSecurity((state) => seen.push(state));
    expect(order.indexOf("plugin:event|listen")).toBeLessThan(
      order.indexOf("list_security_states"),
    );
    expect(seen).toEqual([EVENT]);
    await emit(LINK_EVENTS.security, {
      ...EVENT,
      seq: 5,
      alert: { own: false, since: null, others: null },
    });
    expect(seen).toHaveLength(2);
    expect(seen[1]).toMatchObject({ seq: 5, alert: { own: false } });
    off();
    await emit(LINK_EVENTS.security, { ...EVENT, seq: 6 });
    expect(seen).toHaveLength(2);
  });

  it("lets go of the listener when the replay fails", async () => {
    ipc((cmd) => {
      if (cmd === "list_security_states") throw new Error("pont en panne");
      return null;
    });
    const bridge = new TauriLinkBridge();
    await expect(bridge.onSecurity(() => {})).rejects.toThrow();
  });
});
