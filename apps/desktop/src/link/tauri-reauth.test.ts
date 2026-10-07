import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";

// HRT-30 : le test de garde côté interface. Toute action d'administration du pont réel envoie son mot de
// passe de confirmation à la coquille (qui ajoute la preuve de la clé de CE PC) ; aucune n'a de forme sans
// lui. La liste est fermée : une action de plus au pont doit y figurer. La même liste est tenue côté Rust
// (`apps/desktop/src-tauri/tests/capabilities.rs`, `ADMIN_COMMANDS`).

type Call = { cmd: string; args: Record<string, unknown> };

function ipc(handler: (cmd: string) => unknown) {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    return handler(cmd);
  });
  return calls;
}

afterEach(() => clearMocks());

const SECRET = "Confirm-Pass-12";
const done = { kind: "done", account: null, sessions_closed: 0 };

describe("le pont réel envoie le mot de passe de confirmation de chaque acte d'administration", () => {
  it("passes it on the nine acts that carry one, and never as part of a free route", async () => {
    const calls = ipc((cmd) => {
      if (cmd === "update_agent") return { kind: "accepted", version: "0.2.0" };
      if (cmd === "set_attack_mode") {
        return {
          kind: "done",
          attack_mode: { state: "active", since: null, resumesInS: null, lastEnd: null },
        };
      }
      if (cmd === "set_reauth_setting") return { kind: "done", mode: "each" };
      return done;
    });
    const bridge = new TauriLinkBridge();
    await bridge.createAccount("s1", "paul", "Next-Pass-12345", "readonly", SECRET);
    await bridge.changeAccountRole("s1", "A1", "admin", SECRET);
    await bridge.setAccountPassword("s1", "A1", "Next-Pass-12345", SECRET);
    await bridge.deleteAccount("s1", "A1", null, SECRET);
    await bridge.closeAccountSessions("s1", "A1", SECRET);
    await bridge.updateAgent("s1", "0.2.0", SECRET);
    await bridge.setAttackMode("s1", true, SECRET);
    await bridge.changeOwnPassword("s1", SECRET, "Next-Pass-12345", false);
    await bridge.setReauthSetting("s1", "each", SECRET);
    const carriers: Record<string, string> = {
      create_account: "adminPassword",
      change_account_role: "adminPassword",
      set_account_password: "adminPassword",
      delete_account: "adminPassword",
      close_account_sessions: "adminPassword",
      update_agent: "adminPassword",
      set_attack_mode: "password",
      change_own_password: "current",
      set_reauth_setting: "password",
    };
    expect(calls.map((call) => call.cmd).sort()).toEqual(Object.keys(carriers).sort());
    for (const call of calls) {
      const key = carriers[call.cmd] as string;
      expect(call.args[key], call.cmd).toBe(SECRET);
    }
  });

  it("lets the delay of 5 minutes send no password only for the acts that can be covered", async () => {
    const calls = ipc(() => done);
    const bridge = new TauriLinkBridge();
    await bridge.createAccount("s1", "paul", "Next-Pass-12345", "readonly", null);
    await bridge.changeAccountRole("s1", "A1", "readonly", null);
    await bridge.deleteAccount("s1", "A1", null, null);
    await bridge.closeAccountSessions("s1", "A1", null);
    expect(calls.map((call) => call.args.adminPassword)).toEqual([null, null, null, null]);
    // Le mot de passe d'un autre, la mise à jour de l'agent, le mode attaque et le réglage n'ont AUCUNE
    // forme sans mot de passe : leur paramètre est une chaîne (le compilateur de l'interface l'impose).
    type Required = Parameters<TauriLinkBridge["setAccountPassword"]>[3] &
      Parameters<TauriLinkBridge["updateAgent"]>[2];
    const required: Required = "x";
    expect(required).toBe("x");
  });

  it("reads the state of the confirmation and the coverage, and turns the setting outcome", async () => {
    const calls = ipc((cmd) => {
      if (cmd === "get_reauth_state") {
        return {
          supported: true,
          required: true,
          mode: "window",
          elevatedForS: 120,
          hasDeviceKey: true,
        };
      }
      if (cmd === "reauth_covers") return true;
      return { kind: "unknown", op_id: "op-1" };
    });
    const bridge = new TauriLinkBridge();
    expect((await bridge.getReauthState("s1")).elevatedForS).toBe(120);
    expect(await bridge.reauthCovers("account_delete", null)).toBe(true);
    expect(await bridge.setReauthSetting("s1", "each", SECRET)).toEqual({
      kind: "unknown",
      opId: "op-1",
    });
    expect(calls[0]).toEqual({ cmd: "get_reauth_state", args: { serverId: "s1" } });
    expect(calls[1]).toEqual({
      cmd: "reauth_covers",
      args: { kind: "account_delete", role: null },
    });
  });
});
