import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { LinkStateDto, ServerDto } from "@/bindings";
import { LINK_EVENTS, TauriLinkBridge, toServerInfo, toStateEvent } from "./tauri";
import { LinkCommandError } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };

const forge: ServerDto = {
  id: "01J9",
  name: "Forge",
  address: "192.168.1.20",
  host: "192.168.1.20",
  port: 7341,
  color: 3,
  role: "admin",
  username: "marie",
  remember: true,
};

const state: LinkStateDto = {
  serverId: "01J9",
  seq: 4,
  state: "offline",
  since: 1000,
  lastContactAt: 900,
  nextRetryAt: null,
  blocked: "fingerprint_changed",
  reason: null,
  failedAttempts: 2,
};

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

describe("conversions", () => {
  it("keeps a server and falls back to the first color for an unknown number", () => {
    expect(toServerInfo(forge)).toMatchObject({
      id: "01J9",
      color: 3,
      role: "admin",
      remember: true,
    });
    expect(toServerInfo({ ...forge, color: 42 }).color).toBe(1);
  });

  it("turns a state into an event and dates an unknown start with the clock", () => {
    expect(toStateEvent(state)).toMatchObject({
      seq: 4,
      blocked: "fingerprint_changed",
      failedAttempts: 2,
    });
    expect(toStateEvent({ ...state, since: null }, () => 77).since).toBe(77);
  });
});

describe("TauriLinkBridge", () => {
  it("subscribes first, then replays the servers and the states of the shell", async () => {
    const calls = ipc((cmd) =>
      cmd === "list_servers" ? [forge] : cmd === "list_link_states" ? [state] : undefined,
    );
    const bridge = new TauriLinkBridge();
    const lists: string[][] = [];
    const states: number[] = [];
    const off = await bridge.onServersChanged((servers) => lists.push(servers.map((s) => s.name)));
    const offState = await bridge.onLinkState((event) => states.push(event.seq));
    expect(lists).toEqual([["Forge"]]);
    expect(states).toEqual([4]);
    // L'écoute est posée avant la lecture de l'instantané : plus de fenêtre où un ajout se perd.
    const order = calls.map((call) => call.cmd);
    expect(order.indexOf("plugin:event|listen")).toBeLessThan(order.indexOf("list_servers"));
    // Un événement ensuite.
    await emit(LINK_EVENTS.servers, { servers: [forge, { ...forge, id: "02", name: "Salon" }] });
    await emit(LINK_EVENTS.state, { ...state, seq: 5, state: "connected", blocked: null });
    expect(lists.at(-1)).toEqual(["Forge", "Salon"]);
    expect(states).toEqual([4, 5]);
    off();
    offState();
    await emit(LINK_EVENTS.state, { ...state, seq: 6 });
    expect(states).toEqual([4, 5]);
  });

  it("delivers the fingerprint alert, the operation outcomes and the notices", async () => {
    ipc(() => undefined);
    const bridge = new TauriLinkBridge();
    const got: string[] = [];
    await bridge.onFingerprintChanged((c) => got.push(`fp:${c.presentedHex}`));
    await bridge.onOperation((o) => got.push(`op:${o.outcome}`));
    await bridge.onNotice((n) => got.push(`notice:${n.kind}`));
    await emit(LINK_EVENTS.fingerprint, {
      serverId: "01J9",
      expected: "AAAA",
      presented: "BBBB",
      presentedHex: "bb".repeat(32),
    });
    await emit(LINK_EVENTS.operation, { opId: "o1", serverId: "01J9", outcome: "unknown" });
    await emit(LINK_EVENTS.notice, { kind: "operations_lost", serverId: "01J9" });
    expect(got).toEqual([`fp:${"bb".repeat(32)}`, "op:unknown", "notice:operations_lost"]);
  });

  it("sends each command with the arguments of the shell and returns typed results", async () => {
    const calls = ipc((cmd) => {
      switch (cmd) {
        case "probe_server":
          return {
            fingerprint: "ab".repeat(32),
            display: "ABAB ABAB ABAB ABAB ABAB ABAB ABAB ABAB",
            machineName: "forge",
            agentVersion: "0.1.0",
            macAddresses: ["AA:BB"],
          };
        case "add_server":
        case "update_server":
          return forge;
        case "login":
          return { role: "readonly", username: "marie" };
        default:
          return null;
      }
    });
    const bridge = new TauriLinkBridge();
    expect((await bridge.probeServer("forge.lan", null)).machineName).toBe("forge");
    const added = await bridge.addServer({
      name: "Forge",
      color: 3,
      host: "forge.lan",
      port: null,
      fingerprint: "ab".repeat(32),
      macAddresses: ["AA:BB"],
    });
    expect(added.id).toBe("01J9");
    expect(await bridge.login("01J9", "marie", "Mot-de-passe-1", true)).toEqual({
      role: "readonly",
    });
    await bridge.logout("01J9");
    await bridge.retryNow("01J9");
    await bridge.acceptFingerprint("01J9", "cd".repeat(32));
    await bridge.updateServer("01J9", {
      name: "Forge",
      color: 3,
      host: "x.lan",
      port: 7443,
      fingerprint: "cd".repeat(32),
    });
    await bridge.removeServer("01J9");
    await bridge.forgetCredentials("01J9");
    expect(calls.map((c) => c.cmd)).toEqual([
      "probe_server",
      "add_server",
      "login",
      "logout",
      "retry_now",
      "accept_fingerprint",
      "update_server",
      "remove_server",
      "forget_credentials",
    ]);
    expect(calls[0]?.args).toEqual({ host: "forge.lan", port: null });
    expect(calls[1]?.args).toEqual({
      name: "Forge",
      color: 3,
      host: "forge.lan",
      port: null,
      fingerprint: "ab".repeat(32),
      macAddresses: ["AA:BB"],
    });
    expect(calls[2]?.args).toEqual({
      serverId: "01J9",
      username: "marie",
      password: "Mot-de-passe-1",
      remember: true,
    });
  });

  it("rejects a failed command with the typed failure", async () => {
    ipc((cmd) => {
      if (cmd === "login") throw { kind: "too_many_attempts", retry_after_s: 90 };
      if (cmd === "probe_server") throw { kind: "not_agent" };
      return null;
    });
    const bridge = new TauriLinkBridge();
    const refused = await bridge.login("01J9", "marie", "x", false).catch((e: unknown) => e);
    expect(refused).toBeInstanceOf(LinkCommandError);
    expect((refused as LinkCommandError).failure).toEqual({
      kind: "too_many_attempts",
      retry_after_s: 90,
    });
    const probe = await bridge.probeServer("x", null).catch((e: unknown) => e);
    expect((probe as LinkCommandError).failure.kind).toBe("not_agent");
  });

  it("gives up cleanly when the snapshot cannot be read", async () => {
    ipc((cmd) => {
      if (cmd === "list_servers") throw new Error("coquille en panne");
      return undefined;
    });
    const bridge = new TauriLinkBridge();
    await expect(bridge.onServersChanged(() => {})).rejects.toThrow("coquille en panne");
  });
});
