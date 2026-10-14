import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ServiceLogBody } from "@/bindings";
import type { LogMessage } from "./logs";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";

// Le pont RÉEL des journaux contre la forme que la coquille écrit sur `link://service-logs` (voir
// `src-tauri/tests/service_logs_runtime.rs`). Ce qui n'est pas prouvé ici : l'émission réelle vers la WebView.
type Call = { cmd: string; args: Record<string, unknown> };

function ipc(handler: (cmd: string) => unknown) {
  const calls: Call[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      return handler(cmd);
    },
    { shouldMockEvents: true },
  );
  return calls;
}

afterEach(() => clearMocks());

const send = (session: string, body: ServiceLogBody) =>
  emit(LINK_EVENTS.serviceLogs, { session, body });

describe("journaux : le pont réel", () => {
  it("ouvre par une commande typée, ne rend que les messages de SA session, accuse, ferme", async () => {
    const calls = ipc((cmd) =>
      cmd === "open_service_logs" ? "log-7" : cmd === "announce_log_page" ? 3 : null,
    );
    const bridge = new TauriLinkBridge();
    const seen: LogMessage[] = [];
    const reading = await bridge.openServiceLogs("forge", "c_web", (m) => seen.push(m));
    // La page s'annonce d'abord (la coquille ferme ce qu'une page précédente a laissé), puis l'ouverture porte sa génération.
    expect(calls[0]).toEqual({ cmd: "announce_log_page", args: {} });
    expect(calls[1]).toEqual({
      cmd: "open_service_logs",
      args: { page: 3, serverId: "forge", key: "c_web" },
    });

    await send("log-7", {
      kind: "status",
      state: "live",
      container: null,
      mixed: false,
      exitCode: null,
      endedAt: null,
      notKept: false,
    });
    await send("log-7", {
      kind: "lines",
      seq: 1,
      items: [{ t: 5, s: "err", x: "boum", cut: null }],
    });
    await send("log-8", {
      kind: "lines",
      seq: 1,
      items: [{ t: 6, s: "out", x: "autre onglet", cut: null }],
    });
    await send("log-7", { kind: "gap", count: null, reason: "resume_lost" });
    await vi.waitFor(() => expect(seen).toHaveLength(3));
    expect(seen[0]).toMatchObject({ kind: "status", state: "live" });
    expect(seen[1]).toEqual({
      kind: "lines",
      seq: 1,
      lines: [{ t: 5, source: "err", text: "boum", cut: null }],
    });
    expect(seen[2]).toEqual({ kind: "gap", count: null, reason: "resume_lost" });

    reading.ack(2);
    reading.close();
    reading.close();
    await vi.waitFor(() => expect(calls.some((c) => c.cmd === "close_service_logs")).toBe(true));
    expect(calls.filter((c) => c.cmd === "ack_service_logs")).toEqual([
      { cmd: "ack_service_logs", args: { session: "log-7", batches: 2 } },
    ]);
    expect(calls.filter((c) => c.cmd === "close_service_logs")).toHaveLength(1);
    // Fermée : plus rien n'est rendu.
    await send("log-7", { kind: "interrupted" });
    await new Promise((r) => setTimeout(r, 30));
    expect(seen).toHaveLength(3);
  });

  it("la page s'annonce une seule fois, puis chaque ouverture porte sa génération", async () => {
    const calls = ipc((cmd) =>
      cmd === "open_service_logs" ? "log-1" : cmd === "announce_log_page" ? 9 : null,
    );
    const bridge = new TauriLinkBridge();
    await bridge.openServiceLogs("forge", "c_web", () => undefined);
    await bridge.openServiceLogs("forge", "c_db", () => undefined);
    expect(calls.filter((c) => c.cmd === "announce_log_page")).toHaveLength(1);
    expect(calls.filter((c) => c.cmd === "open_service_logs").map((c) => c.args.page)).toEqual([
      9, 9,
    ]);
  });

  it("une génération inconnue de la coquille : la page s'annonce de nouveau et réessaie une fois", async () => {
    let generation = 0;
    let refused = false;
    const calls = ipc((cmd) => {
      if (cmd === "announce_log_page") return ++generation;
      if (cmd === "open_service_logs") {
        if (!refused) {
          refused = true;
          throw { kind: "log_page_unknown" };
        }
        return "log-4";
      }
      return null;
    });
    const bridge = new TauriLinkBridge();
    await bridge.openServiceLogs("forge", "c_web", () => undefined);
    expect(calls.filter((c) => c.cmd === "announce_log_page")).toHaveLength(2);
    expect(calls.filter((c) => c.cmd === "open_service_logs").map((c) => c.args.page)).toEqual([
      1, 2,
    ]);
  });

  it("une fermeture annoncée par la coquille arrive à l'écran, et la page n'a plus rien à fermer ni à battre", async () => {
    const calls = ipc((cmd) =>
      cmd === "open_service_logs" ? "log-7" : cmd === "announce_log_page" ? 1 : [],
    );
    const bridge = new TauriLinkBridge(Date.now, 15);
    const seen: LogMessage[] = [];
    const reading = await bridge.openServiceLogs("forge", "c_web", (m) => seen.push(m));
    await send("log-7", { kind: "ended", end: { kind: "closed", cause: "window_hidden" } });
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]).toEqual({ kind: "ended", end: { kind: "closed", cause: "window_hidden" } });
    reading.close();
    await new Promise((r) => setTimeout(r, 60));
    expect(calls.some((c) => c.cmd === "close_service_logs")).toBe(false);
    expect(calls.some((c) => c.cmd === "keep_service_logs")).toBe(false);
  });

  it("le battement : la page signale sa présence ; une session que la coquille ne tient plus est dite perdue", async () => {
    let alive: string[] = ["log-7"];
    const calls = ipc((cmd) => {
      if (cmd === "open_service_logs") return "log-7";
      if (cmd === "announce_log_page") return 1;
      if (cmd === "keep_service_logs") return alive;
      return null;
    });
    const bridge = new TauriLinkBridge(Date.now, 15);
    const seen: LogMessage[] = [];
    const reading = await bridge.openServiceLogs("forge", "c_web", (m) => seen.push(m));
    await vi.waitFor(() =>
      expect(calls.filter((c) => c.cmd === "keep_service_logs").length).toBeGreaterThan(1),
    );
    expect(calls.find((c) => c.cmd === "keep_service_logs")?.args).toEqual({ page: 1 });
    expect(seen).toHaveLength(0);
    // La coquille a fermé la session (événement perdu) : le battement le révèle.
    alive = [];
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]).toEqual({ kind: "ended", end: { kind: "closed", cause: "lost" } });
    // Plus de battement une fois la session finie.
    const beats = calls.filter((c) => c.cmd === "keep_service_logs").length;
    await new Promise((r) => setTimeout(r, 60));
    expect(calls.filter((c) => c.cmd === "keep_service_logs").length).toBe(beats);
    reading.close();
  });

  it("un message d'un genre inconnu est ignoré, une fin inconnue est générique", async () => {
    ipc((cmd) => (cmd === "open_service_logs" ? "log-1" : null));
    const bridge = new TauriLinkBridge();
    const seen: LogMessage[] = [];
    await bridge.openServiceLogs("forge", "c_web", (m) => seen.push(m));
    await send("log-1", { kind: "progress" } as unknown as ServiceLogBody);
    await send("log-1", { kind: "ended", end: { kind: "demain" } } as unknown as ServiceLogBody);
    await vi.waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]).toEqual({ kind: "ended", end: { kind: "unknown" } });
  });

  it("un échec d'ouverture se rend typé et libère l'écoute", async () => {
    ipc((cmd) => {
      if (cmd === "open_service_logs") throw { kind: "not_connected" };
      return null;
    });
    const bridge = new TauriLinkBridge();
    await expect(bridge.openServiceLogs("forge", "c_web", () => undefined)).rejects.toBeDefined();
  });
});
