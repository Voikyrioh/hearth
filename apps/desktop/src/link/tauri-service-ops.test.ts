import { readFileSync } from "node:fs";
import { join } from "node:path";
import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { ServiceOperationEvent as Wire } from "@/bindings";
import { setLinkBridge } from "@/link";
import type { ServiceOperationEvent } from "./serviceOps";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Le pont RÉEL contre la fiche de référence de la coquille : `src-tauri/tests/fixtures/service-operation-event.json`
// est exactement ce que la coquille Rust écrit sur `link://service-operations` (elle la compare, champ par champ, à
// la conversion de `hearth_proto::api::service_ops`, tests/service_ops_runtime.rs). Ici la même fiche traverse l'IPC
// simulé de Tauri et le pont réel. L'émission réelle de la fenêtre et l'écoute de la WebView ne sont pas prouvées ici.
const fixture = JSON.parse(
  readFileSync(
    join(process.cwd(), "src-tauri", "tests", "fixtures", "service-operation-event.json"),
    "utf8",
  ),
) as { running: Wire; awaiting: Wire; gone: Wire };

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

describe("l'abonnement aux opérations du pont réel", () => {
  it("écoute d'abord, rejoue ce que la coquille tient, puis suit les changements", async () => {
    const order: string[] = [];
    const seen: ServiceOperationEvent[] = [];
    ipc((cmd) => {
      order.push(cmd);
      return cmd === "list_service_operations" ? [fixture.running] : null;
    });
    const off = await new TauriLinkBridge().onServiceOperations((event) => seen.push(event));
    expect(order.indexOf("plugin:event|listen")).toBeLessThan(
      order.indexOf("list_service_operations"),
    );
    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({
      kind: "upsert",
      serverId: "forge",
      seq: 1,
      operation: {
        id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
        kind: "deploy",
        status: "running",
        percent: 40,
        bytesDone: 400,
      },
    });
    await emit(LINK_EVENTS.serviceOperations, fixture.awaiting);
    await emit(LINK_EVENTS.serviceOperations, fixture.gone);
    expect(seen.map((event) => event.kind)).toEqual(["upsert", "upsert", "gone"]);
    // Le choix inconnu n'existe pas côté interface, la décision garde ceux qu'elle connaît.
    expect(seen[1]).toMatchObject({
      operation: { decision: { choices: ["revert", "keep"], expiresAt: 1791972900000 } },
    });
    off();
  });

  it("un choix que ce client ne connaît pas est écarté (jamais proposé)", async () => {
    const seen: ServiceOperationEvent[] = [];
    ipc((cmd) => (cmd === "list_service_operations" ? [] : null));
    const off = await new TauriLinkBridge().onServiceOperations((event) => seen.push(event));
    const future = structuredClone(fixture.awaiting);
    if (future.change.kind === "upsert" && future.change.operation.decision) {
      future.change.operation.decision.choices = ["keep", "unknown"];
    }
    await emit(LINK_EVENTS.serviceOperations, future);
    expect(seen[0]).toMatchObject({ operation: { decision: { choices: ["keep"] } } });
    off();
  });
});

describe("les commandes des opérations du pont réel", () => {
  it("envoie UNE commande typée par geste, sans paramètre libre", async () => {
    const calls = ipc((cmd) => {
      if (cmd === "decide_service_operation") {
        return { kind: "refused", refusal: { kind: "too_many_attempts", retry_after_s: 30 } };
      }
      return null;
    });
    const bridge = new TauriLinkBridge();
    await bridge.ackServiceOperations("forge", ["A", "B"]);
    const outcome = await bridge.decideServiceOperation("forge", "A", "keep", "secret-ci");
    expect(outcome).toEqual({
      kind: "refused",
      refusal: { kind: "too_many_attempts", retry_after_s: 30 },
    });
    expect(calls).toEqual([
      { cmd: "ack_service_operations", args: { serverId: "forge", opIds: ["A", "B"] } },
      {
        cmd: "decide_service_operation",
        args: { serverId: "forge", opId: "A", choice: "keep", adminPassword: "secret-ci" },
      },
    ]);
    for (const call of calls) {
      for (const free of ["path", "method", "body", "url", "command"]) {
        expect(Object.keys(call.args)).not.toContain(free);
      }
    }
  });

  it("lit l'issue d'une décision réussie et d'un résultat inconnu", async () => {
    ipc(() => ({ kind: "unknown", opId: "sim-op-1" }));
    const bridge = new TauriLinkBridge();
    expect(await bridge.decideServiceOperation("forge", "A", "keep", null)).toEqual({
      kind: "unknown",
      opId: "sim-op-1",
    });
  });

  it("rejette avec l'échec typé quand le droit manque", async () => {
    ipc(() => {
      throw { kind: "missing_services_right" };
    });
    const error = await new TauriLinkBridge()
      .decideServiceOperation("forge", "A", "keep", "x")
      .catch((e) => e);
    expect(error).toBeInstanceOf(LinkCommandError);
    expect((error as LinkCommandError).failure.kind).toBe("missing_services_right");
  });
});
