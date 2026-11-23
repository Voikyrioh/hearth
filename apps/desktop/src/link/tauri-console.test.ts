import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";

// La console et les joueurs connectés (HRT-291) : le pont RÉEL contre l'IPC simulé de Tauri. La classe signée et le mot
// de passe partent avec le texte ; la coquille Rust lit les refus par leur code (tests de `service_ops/console.rs`).
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

describe("le pont réel, console", () => {
  it("décrit la console : les mots du fil inconnus se lisent « inconnu »", async () => {
    const calls = ipc(() => ({
      kind: "ready",
      view: {
        available: true,
        unavailableReason: null,
        state: "melting",
        canWrite: true,
        policy: {
          refused: [{ verb: "stop", reason: "use_hearth_stop" }],
          sensitive: [{ verb: "op", title: "t", sentence: "s", target: "first_arg" }],
        },
        quick: [{ id: "save", label: "Enregistrer le monde", text: "save-all", mode: "send" }],
        limits: { maxChars: 256, perMinute: 20, replyTimeoutS: 10 },
      },
    }));
    const outcome = await new TauriLinkBridge().describeConsole("forge", "h_mc");
    expect(calls).toEqual([{ cmd: "describe_console", args: { serverId: "forge", key: "h_mc" } }]);
    if (outcome.kind !== "ready") throw new Error("vue attendue");
    expect(outcome.view.state).toBe("unknown");
    expect(outcome.view.policy.sensitive[0]?.target).toBe("first_arg");
    expect(outcome.view.limits?.perMinute).toBe(20);
  });

  it("lit les joueurs : un état inconnu ne donne aucun chiffre, un nom absent devient une liste vide", async () => {
    ipc(() => ({
      kind: "ready",
      view: {
        state: "asleep",
        online: null,
        max: null,
        names: null,
        hiddenNames: 0,
        moreNames: 0,
        readAt: null,
      },
    }));
    const outcome = await new TauriLinkBridge().readPlayers("forge", "h_mc");
    expect(outcome).toMatchObject({
      kind: "ready",
      view: { state: "unknown", online: null, names: [] },
    });
  });

  it("envoie une commande : le texte, la classe et le mot de passe partent ; la réponse revient", async () => {
    const calls = ipc(() => ({
      kind: "replied",
      reply: { outcome: "answered", reply: "Made Bram a server operator", truncated: false },
    }));
    const outcome = await new TauriLinkBridge().sendConsole(
      "forge",
      "h_mc",
      "op Bram",
      2,
      "secret",
    );
    expect(calls[0]).toEqual({
      cmd: "send_console",
      args: {
        serverId: "forge",
        key: "h_mc",
        command: "op Bram",
        commandClass: 2,
        adminPassword: "secret",
      },
    });
    expect(outcome).toMatchObject({
      kind: "replied",
      reply: { outcome: "answered", reply: "Made Bram a server operator" },
    });
  });

  it("un refus, et un résultat inconnu (jamais renvoyé), arrivent tels quels", async () => {
    ipc(() => ({ kind: "refused", refusal: { kind: "rate_limited", retry_after_s: 12 } }));
    expect(await new TauriLinkBridge().sendConsole("forge", "h_mc", "list", 1, null)).toEqual({
      kind: "refused",
      refusal: { kind: "rate_limited", retry_after_s: 12 },
    });
    clearMocks();
    ipc(() => ({ kind: "unknown", opId: "x" }));
    expect(await new TauriLinkBridge().sendConsole("forge", "h_mc", "list", 1, null)).toEqual({
      kind: "unknown",
      opId: "x",
    });
  });
});
