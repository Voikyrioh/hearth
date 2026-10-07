import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };

function ipc(handler: (cmd: string, args: Record<string, unknown>) => unknown) {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: payload });
    return handler(cmd, payload);
  });
  return calls;
}

afterEach(() => clearMocks());

const DEVICE = {
  id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
  name: "salon/0.1.0",
  createdAt: "2026-10-07T00:12:03.100Z",
  lastProvedAt: "2026-10-07T08:30:15.250Z",
  lastAddr: "192.168.1.20",
  current: true,
};

describe("trusted device commands of the real bridge", () => {
  it("sends ONE typed command per call, with business parameters only", async () => {
    const calls = ipc((cmd) =>
      cmd === "list_trusted_devices"
        ? { kind: "listed", devices: [DEVICE], max: 8 }
        : { kind: "done" },
    );
    const bridge = new TauriLinkBridge();
    await bridge.listTrustedDevices("s1");
    await bridge.removeTrustedDevice("s1", "D1", "Mot-De-Passe-12");
    expect(calls).toEqual([
      { cmd: "list_trusted_devices", args: { serverId: "s1" } },
      {
        cmd: "remove_trusted_device",
        args: { serverId: "s1", deviceId: "D1", password: "Mot-De-Passe-12" },
      },
    ]);
    // Aucune route, aucune méthode, aucun corps, aucune clé : jamais un paramètre libre.
    for (const call of calls) {
      expect(Object.keys(call.args).sort()).not.toEqual(expect.arrayContaining(["path"]));
    }
  });

  it("reads the list, and an agent without the feature as unsupported", async () => {
    ipc(() => ({ kind: "listed", devices: [DEVICE], max: 8 }));
    const bridge = new TauriLinkBridge();
    expect(await bridge.listTrustedDevices("s1")).toEqual({
      kind: "listed",
      devices: [DEVICE],
      max: 8,
    });
    clearMocks();
    ipc(() => ({ kind: "unsupported" }));
    expect(await bridge.listTrustedDevices("s1")).toEqual({ kind: "unsupported" });
  });

  it("reads every outcome of a removal, the unknown one with its operation key", async () => {
    const bridge = new TauriLinkBridge();
    ipc(() => ({ kind: "done" }));
    expect(await bridge.removeTrustedDevice("s1", "D1", "x")).toEqual({ kind: "done" });
    clearMocks();
    ipc(() => ({ kind: "refused", refusal: { kind: "wrong_password" } }));
    expect(await bridge.removeTrustedDevice("s1", "D1", "x")).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    clearMocks();
    ipc(() => ({ kind: "unknown", op_id: "OP1" }));
    expect(await bridge.removeTrustedDevice("s1", "D1", "x")).toEqual({
      kind: "unknown",
      opId: "OP1",
    });
  });

  it("rejects with the typed failure when the command fails", async () => {
    ipc(() => {
      throw { kind: "not_connected" };
    });
    const bridge = new TauriLinkBridge();
    await expect(bridge.listTrustedDevices("s1")).rejects.toBeInstanceOf(LinkCommandError);
  });
});
