import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge, toAccountOutcome } from "./tauri";
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

const done = { kind: "done", account: null, sessions_closed: 2 };

describe("account commands of the real bridge", () => {
  it("sends ONE typed command per action, with business parameters only", async () => {
    const calls = ipc((cmd) =>
      cmd === "check_account_input" ? { username: null, password: [] } : done,
    );
    const bridge = new TauriLinkBridge();
    await bridge.checkAccountInput("marie", "Secret-Pass-12");
    await bridge.createAccount("s1", "paul", "Secret-Pass-12", "readonly");
    await bridge.changeAccountRole("s1", "A1", "admin");
    await bridge.setAccountPassword("s1", "A1", "Secret-Pass-12");
    await bridge.changeOwnPassword("s1", "Old-Secret-12", "Secret-Pass-12");
    await bridge.closeAccountSessions("s1", "A1");
    await bridge.deleteAccount("s1", "A1", "marie");
    expect(calls).toEqual([
      { cmd: "check_account_input", args: { username: "marie", password: "Secret-Pass-12" } },
      {
        cmd: "create_account",
        args: { serverId: "s1", username: "paul", password: "Secret-Pass-12", role: "readonly" },
      },
      { cmd: "change_account_role", args: { serverId: "s1", accountId: "A1", role: "admin" } },
      {
        cmd: "set_account_password",
        args: { serverId: "s1", accountId: "A1", password: "Secret-Pass-12" },
      },
      {
        cmd: "change_own_password",
        args: { serverId: "s1", current: "Old-Secret-12", password: "Secret-Pass-12" },
      },
      { cmd: "close_account_sessions", args: { serverId: "s1", accountId: "A1" } },
      { cmd: "delete_account", args: { serverId: "s1", accountId: "A1", confirmation: "marie" } },
    ]);
    // Jamais un chemin, une méthode ou un corps libre vers l'agent.
    for (const call of calls) {
      for (const free of ["path", "method", "body", "url", "headers"]) {
        expect(Object.keys(call.args)).not.toContain(free);
      }
    }
  });

  it("reads the list, and a refusal of the agent is a value, not an exception", async () => {
    ipc(() => ({ kind: "refused", refusal: { kind: "forbidden" } }));
    const list = await new TauriLinkBridge().listAccounts("s1");
    expect(list).toEqual({ kind: "refused", refusal: { kind: "forbidden" } });
  });

  it("turns a failure of the command into a typed error", async () => {
    mockIPC(() => {
      throw { kind: "not_connected" };
    });
    await expect(new TauriLinkBridge().createAccount("s1", "paul", "x", "admin")).rejects.toEqual(
      new LinkCommandError({ kind: "not_connected" }),
    );
  });
});

describe("outcomes", () => {
  it("renames the fields for the interface and keeps refusals as they are", () => {
    expect(toAccountOutcome(done as never)).toEqual({
      kind: "done",
      account: null,
      sessionsClosed: 2,
    });
    expect(toAccountOutcome({ kind: "unknown", op_id: "OP1" })).toEqual({
      kind: "unknown",
      opId: "OP1",
    });
    const refused = { kind: "refused", refusal: { kind: "last_admin" } } as const;
    expect(toAccountOutcome(refused)).toEqual(refused);
  });
});
