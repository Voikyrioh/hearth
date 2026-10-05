import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
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

  it("delivers the fingerprint alert, the operation outcomes and the notices live", async () => {
    ipc(() => []);
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
    await emit(LINK_EVENTS.notice, { id: 3, kind: "operations_lost", serverId: "01J9" });
    expect(got).toEqual([`fp:${"bb".repeat(32)}`, "op:unknown", "notice:operations_lost"]);
  });

  it("replays what the shell kept for a listener that arrives after the event (review PR 12)", async () => {
    const alert = {
      serverId: "01J9",
      expected: "AAAA",
      presented: "BBBB",
      presentedHex: "bb".repeat(32),
    };
    const calls = ipc((cmd) => {
      switch (cmd) {
        case "list_fingerprint_alerts":
          return [alert];
        case "list_link_notices":
          return [{ id: 7, kind: "operations_lost", serverId: "01J9" }];
        case "list_unread_operations":
          return [{ opId: "o1", serverId: "01J9", outcome: "not_executed" }];
        default:
          return undefined;
      }
    });
    const bridge = new TauriLinkBridge();
    const got: string[] = [];
    await bridge.onFingerprintChanged((c) => got.push(`fp:${c.serverId}`));
    await bridge.onOperation((o) => got.push(`op:${o.outcome}`));
    await bridge.onNotice((n) => got.push(`notice:${n.kind}`));
    expect(got).toEqual(["fp:01J9", "op:not_executed", "notice:operations_lost"]);
    // Remis à l'écouteur, puis acquittés par identifiant (la lecture ne détruit rien).
    await vi.waitFor(() => {
      const acks = calls.filter((call) => call.cmd.startsWith("ack_"));
      expect(acks.map((call) => call.cmd).sort()).toEqual([
        "ack_link_notices",
        "ack_unread_operations",
      ]);
      expect(acks.find((call) => call.cmd === "ack_link_notices")?.args).toEqual({ ids: [7] });
      expect(acks.find((call) => call.cmd === "ack_unread_operations")?.args).toEqual({
        opIds: ["o1"],
      });
    });
    // S'abonner d'abord, puis lire : aucun événement ne tombe entre les deux.
    const order = calls.map((call) => call.cmd);
    expect(order.indexOf("plugin:event|listen")).toBeLessThan(
      order.indexOf("list_fingerprint_alerts"),
    );
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
        case "add_and_login":
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
    const added = await bridge.addAndLogin({
      name: "Forge",
      color: 3,
      host: "forge.lan",
      port: null,
      fingerprint: "ab".repeat(32),
      macAddresses: ["AA:BB"],
      username: "marie",
      password: "Mot-de-passe-1",
      remember: true,
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
      "add_and_login",
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
      input: {
        name: "Forge",
        color: 3,
        host: "forge.lan",
        port: null,
        fingerprint: "ab".repeat(32),
        macAddresses: ["AA:BB"],
        username: "marie",
        password: "Mot-de-passe-1",
        remember: true,
      },
    });
    expect(calls[5]?.args).toEqual({ serverId: "01J9", fingerprint: "cd".repeat(32) });
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

describe("TauriLinkBridge : actions et serveur affiché (HRT-12)", () => {
  it("sends an action to a server and maps both kinds of answer", async () => {
    const calls = ipc((cmd) => {
      if (cmd !== "run_action") return undefined;
      return calls.filter((call) => call.cmd === "run_action").length === 1
        ? { kind: "completed", status: 200, body: '{"ok":true}' }
        : { kind: "unknown", opId: "01OP" };
    });
    const bridge = new TauriLinkBridge();
    const done = await bridge.runAction("01J9", {
      method: "PUT",
      path: "/me/password",
      body: "{}",
    });
    expect(done).toEqual({ kind: "completed", status: 200, body: '{"ok":true}' });
    const lost = await bridge.runAction("01J9", { method: "DELETE", path: "/x" });
    expect(lost).toEqual({ kind: "unknown", opId: "01OP" });
    expect(calls.filter((call) => call.cmd === "run_action").map((call) => call.args)).toEqual([
      { serverId: "01J9", action: { method: "PUT", path: "/me/password", body: "{}" } },
      { serverId: "01J9", action: { method: "DELETE", path: "/x", body: null } },
    ]);
  });

  it("refuses with the typed failure when the link is not connected", async () => {
    ipc((cmd) => {
      if (cmd === "run_action") throw { kind: "not_connected" };
      return undefined;
    });
    const refused = await new TauriLinkBridge()
      .runAction("01J9", { method: "GET", path: "/x" })
      .catch((error: unknown) => error);
    expect(refused).toBeInstanceOf(LinkCommandError);
    expect((refused as LinkCommandError).failure.kind).toBe("not_connected");
  });

  it("tells the shell which server is displayed", async () => {
    const calls = ipc(() => undefined);
    const bridge = new TauriLinkBridge();
    await bridge.setDisplayedServer("01J9");
    await bridge.setDisplayedServer(null);
    expect(
      calls.filter((call) => call.cmd === "set_displayed_server").map((call) => call.args),
    ).toEqual([{ serverId: "01J9" }, { serverId: null }]);
  });
});
