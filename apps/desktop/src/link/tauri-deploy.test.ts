import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Déployer une image (HRT-69) : le pont RÉEL contre l'IPC simulé de Tauri. Chaque geste est UNE commande typée
// (`deploy_check`, `deploy_service`, `cancel_service_operation`, `read_engine_capacity`) ; la coquille Rust
// construit la requête et lit la réponse (tests de `service_ops/deploy.rs`), le pont ne fait que convertir. Ce
// fichier ne prouve PAS l'appel réel de la coquille (WebView).
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

const request = {
  name: "web",
  image: "nginx",
  tag: "1.27",
  ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp" as const }],
  env: [{ name: "A", value: "v" }],
  command: null,
  resources: { memoryBytes: 1024 ** 3, cpuMillis: 1000 },
  accept: { reservedPorts: false, lowDisk: true },
};

describe("le pont réel, déploiement", () => {
  it("consulte le formulaire sans jamais envoyer une variable ni la commande", async () => {
    const calls = ipc(() => ({
      name: "taken",
      invalidFields: ["image"],
      ports: [
        {
          hostPort: 8080,
          protocol: "tcp",
          status: "held_by_program",
          holder: "sshd",
          suggested: 8081,
          privileged: false,
        },
      ],
      disk: { freeBytes: 2048, low: true },
      imagePresent: false,
    }));
    const check = await new TauriLinkBridge().checkDeployment("forge", request);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.cmd).toBe("deploy_check");
    expect(calls[0]?.args).toEqual({
      serverId: "forge",
      input: {
        name: "web",
        image: "nginx",
        tag: "1.27",
        ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp" }],
      },
    });
    expect(JSON.stringify(calls)).not.toContain('"env"');
    expect(check).toEqual({
      name: "taken",
      invalidFields: ["image"],
      ports: [
        {
          hostPort: 8080,
          protocol: "tcp",
          status: "held_by_program",
          holder: "sshd",
          suggested: 8081,
          privileged: false,
        },
      ],
      disk: { freeBytes: 2048, low: true },
      imagePresent: false,
    });
  });

  it("déploie : la demande entière et le mot de passe partent, l'issue acceptée porte l'opération", async () => {
    const calls = ipc(() => ({ kind: "accepted", operationId: "01OP", serviceKey: "h_01J" }));
    const outcome = await new TauriLinkBridge().deployService("forge", request, "mot de passe");
    expect(outcome).toEqual({ kind: "accepted", operationId: "01OP", serviceKey: "h_01J" });
    expect(calls[0]?.cmd).toBe("deploy_service");
    expect(calls[0]?.args).toEqual({
      serverId: "forge",
      deployment: {
        name: "web",
        image: "nginx",
        tag: "1.27",
        ports: [{ hostPort: 8080, containerPort: 80, protocol: "tcp" }],
        env: [{ name: "A", value: "v" }],
        command: null,
        resources: { memoryBytes: 1024 ** 3, cpuMillis: 1000 },
        accept: { reservedPorts: false, lowDisk: true },
      },
      adminPassword: "mot de passe",
    });
  });

  it("lit un refus par son code et un résultat inconnu sans rien rejouer", async () => {
    const refusal = { kind: "port_in_use", port: 8080, protocol: "tcp", status: "free" };
    ipc(() => ({ kind: "refused", refusal }));
    expect(await new TauriLinkBridge().deployService("forge", request, null)).toEqual({
      kind: "refused",
      refusal,
    });
    const calls = ipc(() => ({ kind: "unknown", opId: "01KEY" }));
    expect(await new TauriLinkBridge().deployService("forge", request, null)).toEqual({
      kind: "unknown",
      opId: "01KEY",
    });
    expect(calls).toHaveLength(1);
  });

  it("une panne de la coquille reste une panne typée, jamais un succès simulé", async () => {
    ipc(() => {
      throw { kind: "missing_services_right" };
    });
    await expect(new TauriLinkBridge().deployService("forge", request, "x")).rejects.toMatchObject({
      failure: { kind: "missing_services_right" },
    });
    await expect(new TauriLinkBridge().deployService("forge", request, "x")).rejects.toBeInstanceOf(
      LinkCommandError,
    );
  });

  it("annule : l'opération rendue est convertie, un refus est lu par son code", async () => {
    const operation = {
      id: "01OP",
      key: "h_01J",
      kind: "deploy",
      status: "cancelling",
      step: "pull",
      steps: [{ step: "pull", state: "running" }],
      percent: 40,
      itemsDone: null,
      bytesDone: null,
      bytesTotal: null,
      etaS: null,
      cancellable: false,
      decision: null,
      cause: null,
    };
    const calls = ipc(() => ({ kind: "done", operation }));
    const done = await new TauriLinkBridge().cancelServiceOperation("forge", "01OP", null);
    expect(calls[0]).toEqual({
      cmd: "cancel_service_operation",
      args: { serverId: "forge", opId: "01OP", adminPassword: null },
    });
    expect(done).toMatchObject({
      kind: "done",
      operation: { id: "01OP", status: "cancelling", cancellable: false },
    });
    ipc(() => ({ kind: "refused", refusal: { kind: "too_late" } }));
    expect(await new TauriLinkBridge().cancelServiceOperation("forge", "01OP", null)).toEqual({
      kind: "refused",
      refusal: { kind: "too_late" },
    });
  });

  it("lit les bornes de la machine ; une réponse hors bornes ne donne aucune borne", async () => {
    const calls = ipc(() => ({
      cpus: 8,
      memoryBytes: 16 * 1024 ** 3,
      reserveBytes: 1024 ** 3,
      managedRunningBytes: 1024 ** 3,
      unmanagedRunningBytes: 2 * 1024 ** 3,
    }));
    expect(await new TauriLinkBridge().readEngineCapacity("forge")).toEqual({
      cpus: 8,
      memoryBytes: 16 * 1024 ** 3,
      reserveBytes: 1024 ** 3,
      committedMemoryBytes: 3 * 1024 ** 3,
    });
    expect(calls[0]).toEqual({ cmd: "read_engine_capacity", args: { serverId: "forge" } });
    ipc(() => ({
      cpus: 0,
      memoryBytes: 0,
      reserveBytes: 0,
      managedRunningBytes: 0,
      unmanagedRunningBytes: 0,
    }));
    expect(await new TauriLinkBridge().readEngineCapacity("forge")).toBeNull();
  });
});
