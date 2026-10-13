import { readFileSync } from "node:fs";
import { join } from "node:path";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ServicesEvent as Wire } from "@/bindings";
import { setLinkBridge } from "@/link";
import { show } from "@/services/view";
import { useServicesStore } from "@/stores/services";
import type { ServicesEvent } from "./services";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Le pont RÉEL contre la fiche de référence de la coquille : `src-tauri/tests/fixtures/services-event.json`
// est exactement ce que la coquille Rust écrit sur `link://services` (elle la compare, champ par champ, à la
// conversion de `hearth_proto::api::services`). Ici, la même fiche traverse l'IPC simulé de Tauri, le pont
// réel et le magasin : c'est le dernier maillon de la chaîne que ce poste peut prouver. L'émission réelle de
// la fenêtre (`window.emit`) et l'écoute de la WebView ne sont pas prouvées ici.
const fixture = JSON.parse(
  readFileSync(
    join(process.cwd(), "src-tauri", "tests", "fixtures", "services-event.json"),
    "utf8",
  ),
) as { supported: Wire; unsupported: Wire; unknown: Wire };

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

beforeEach(() => setActivePinia(createPinia()));
afterEach(() => {
  clearMocks();
  setLinkBridge(null);
});

describe("services commands of the real bridge", () => {
  it("sends ONE typed command per gesture, with no free parameter", async () => {
    const calls = ipc((cmd) => {
      if (cmd === "engine_recheck")
        return fixture.supported.view.kind === "supported" ? fixture.supported.view.engine : null;
      return null;
    });
    const bridge = new TauriLinkBridge();
    const engine = await bridge.recheckEngine("forge");
    await bridge.openEngineDocs();
    expect(calls).toEqual([
      { cmd: "engine_recheck", args: { serverId: "forge" } },
      { cmd: "open_engine_docs", args: {} },
    ]);
    expect(engine).toMatchObject({ state: "ready", name: "Docker Engine - Community" });
    // Ni route, ni méthode, ni corps, ni adresse.
    for (const call of calls) {
      for (const free of ["path", "method", "body", "url", "command", "env"]) {
        expect(Object.keys(call.args)).not.toContain(free);
      }
    }
  });

  it("rejects with the typed failure when the link is not connected", async () => {
    ipc(() => {
      throw { kind: "not_connected" };
    });
    const error = await new TauriLinkBridge().recheckEngine("forge").catch((e) => e);
    expect(error).toBeInstanceOf(LinkCommandError);
    expect((error as LinkCommandError).failure.kind).toBe("not_connected");
  });
});

describe("the services event of the real bridge", () => {
  it("listens first, replays the last known state of each server, then follows the events", async () => {
    const order: string[] = [];
    const seen: ServicesEvent[] = [];
    ipc((cmd) => {
      order.push(cmd);
      return cmd === "list_services_states" ? [fixture.supported] : null;
    });
    const off = await new TauriLinkBridge().onServices((event) => seen.push(event));
    expect(order.indexOf("plugin:event|listen")).toBeLessThan(
      order.indexOf("list_services_states"),
    );
    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ kind: "state", serverId: "forge", seq: 1 });
    await emit(LINK_EVENTS.services, { ...fixture.unsupported, serverId: "forge", seq: 2 });
    expect(seen[1]).toEqual({
      kind: "unsupported",
      serverId: "forge",
      seq: 2,
      agentVersion: "0.1.0",
    });
    off();
    await emit(LINK_EVENTS.services, { ...fixture.unsupported, seq: 3 });
    expect(seen).toHaveLength(2);
  });

  it("reads the values of a newer agent as unknown, from the wire to the store", async () => {
    // La fiche `unknown` est ce que la coquille écrit pour un agent plus récent (test Rust jumeau).
    ipc((cmd) => (cmd === "list_services_states" ? [] : null));
    setLinkBridge(new TauriLinkBridge());
    const store = useServicesStore();
    await store.start();
    await emit(LINK_EVENTS.services, fixture.unknown);
    const shown = show(store.of("futur"));
    expect(shown.kind).toBe("engine");
    if (shown.kind !== "engine") return;
    expect(shown.engine.state).toBe("unknown");
    expect(shown.list.kind).toBe("stale");
    const service = shown.list.kind === "stale" ? shown.list.services[0] : undefined;
    expect(service).toMatchObject({
      origin: "unknown",
      driver: "unknown",
      state: "unknown",
      ports: [{ protocol: "unknown", scope: "unknown" }],
    });
    store.reset();
  });

  it("lets go of the listener when the replay fails", async () => {
    ipc((cmd) => {
      if (cmd === "list_services_states") throw new Error("pont en panne");
      return null;
    });
    await expect(new TauriLinkBridge().onServices(() => {})).rejects.toThrow();
  });
});

describe("the store fed by the real bridge", () => {
  it("shows what the shell sent: the engine, the sorted list, then an agent without the capability", async () => {
    ipc((cmd) => (cmd === "list_services_states" ? [] : null));
    setLinkBridge(new TauriLinkBridge());
    const store = useServicesStore();
    await store.start();
    expect(show(store.of("forge"))).toEqual({ kind: "loading" });

    await emit(LINK_EVENTS.services, fixture.supported);
    const shown = show(store.of("forge"));
    expect(shown.kind).toBe("engine");
    if (shown.kind !== "engine") return;
    expect(shown.engine.state).toBe("ready");
    expect(shown.list.kind === "live" && shown.list.services.map((s) => s.name)).toEqual([
      "web",
      "sans-nom",
    ]);

    // Le même numéro rejoué (signal et lecture qui se croisent) est écarté.
    await emit(LINK_EVENTS.services, { ...fixture.supported, view: fixture.unsupported.view });
    expect(show(store.of("forge")).kind).toBe("engine");
    await emit(LINK_EVENTS.services, {
      ...fixture.unsupported,
      serverId: "forge",
      seq: 5,
    });
    expect(show(store.of("forge"))).toEqual({ kind: "old-agent", agentVersion: "0.1.0" });
    store.reset();
  });
});
