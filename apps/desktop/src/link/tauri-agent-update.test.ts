import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import { toAgentUpdateOutcome } from "./agent-update";
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

const view = {
  current: "0.1.0",
  managed: false,
  inProgress: false,
  progress: null,
  last: null,
  available: { version: "0.2.0" },
};

describe("les commandes de mise à jour de l'agent du pont réel", () => {
  it("lit l'état par UNE commande typée, avec le serveur seulement", async () => {
    const calls = ipc(() => view);
    expect(await new TauriLinkBridge().getAgentUpdate("s1")).toEqual(view);
    expect(calls).toEqual([{ cmd: "get_agent_update", args: { serverId: "s1" } }]);
  });

  it("demande la mise à jour avec le serveur et le NUMÉRO de version, jamais une adresse, une signature ni une somme", async () => {
    const calls = ipc(() => ({ kind: "accepted", version: "0.2.0" }));
    const outcome = await new TauriLinkBridge().updateAgent("s1", "0.2.0", "Admin-Pass-12");
    expect(outcome).toEqual({ kind: "accepted", version: "0.2.0" });
    expect(calls).toEqual([
      {
        cmd: "update_agent",
        args: { serverId: "s1", version: "0.2.0", adminPassword: "Admin-Pass-12" },
      },
    ]);
    for (const free of ["path", "method", "body", "url", "signature", "sha256", "headers"]) {
      expect(Object.keys(calls[0]?.args ?? {})).not.toContain(free);
    }
  });

  it("rend un échec de la commande en erreur typée (refus de rôle de l'agent compris)", async () => {
    mockIPC(() => {
      throw { kind: "forbidden" };
    });
    await expect(new TauriLinkBridge().updateAgent("s1", "0.2.0", "x")).rejects.toEqual(
      new LinkCommandError({ kind: "forbidden" }),
    );
  });

  it("garde un refus de l'agent comme une valeur, et renomme la clé d'opération d'un résultat inconnu", () => {
    const refused = { kind: "refused", refusal: { kind: "managed_install" } } as const;
    expect(toAgentUpdateOutcome(refused)).toEqual(refused);
    expect(toAgentUpdateOutcome({ kind: "unknown", op_id: "OP1" })).toEqual({
      kind: "unknown",
      opId: "OP1",
    });
  });

  it("relaie la progression de TOUS les serveurs (le récepteur les trie) et se désabonne", async () => {
    ipc(() => undefined);
    const bridge = new TauriLinkBridge();
    const seen: string[] = [];
    const off = await bridge.onAgentUpdate((event) =>
      seen.push(`${event.serverId}:${event.progress.step}:${event.progress.percent}`),
    );
    const progress = (step: string, percent: number | null) => ({
      version: "0.2.0",
      step,
      percent,
      outcome: null,
      reason: null,
    });
    await emit(LINK_EVENTS.agentUpdate, { serverId: "s1", progress: progress("download", 35) });
    await emit(LINK_EVENTS.agentUpdate, { serverId: "s2", progress: progress("verify", null) });
    await vi.waitFor(() => expect(seen).toEqual(["s1:download:35", "s2:verify:null"]));
    off();
    await emit(LINK_EVENTS.agentUpdate, { serverId: "s1", progress: progress("install", null) });
    expect(seen).toHaveLength(2);
  });
});
