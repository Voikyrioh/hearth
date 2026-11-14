import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Supprimer un service (HRT-86) : le pont RÉEL contre l'IPC simulé de Tauri. Chaque geste est UNE commande typée
// (`delete_preview`, `delete_service`) ; la coquille Rust construit la requête et lit la réponse par son code (tests de
// `service_ops/delete.rs`), le pont ne fait que convertir. Ce fichier ne prouve PAS l'appel réel de la coquille.
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

const dto = {
  name: "web",
  image: "nginx:1.27",
  running: true,
  stopGraceS: 10,
  containerMissing: false,
  folders: [
    {
      path: "/srv/hearth/data/web",
      bytes: 2048,
      items: 12,
      partial: false,
      unmeasured: false,
      fate: "kept_shared",
      sharedWith: ["base"],
      sharedWithUnmanaged: true,
    },
    {
      path: "/srv/hearth/data/lourd",
      bytes: null,
      items: null,
      partial: true,
      unmeasured: false,
      fate: "erasable",
      sharedWith: [],
      sharedWithUnmanaged: false,
    },
  ],
  serverFolders: ["/mnt/jeux"],
  volumes: 1,
  measuredInTime: false,
  backups: 0,
};

describe("le pont réel, suppression d'un service", () => {
  it("consulte la fenêtre : la clé seule part, la réponse est convertie, un nombre illisible n'est pas lu comme zéro mesuré", async () => {
    const calls = ipc(() => ({ kind: "ready", preview: dto }));
    const outcome = await new TauriLinkBridge().readDeletePreview("forge", "h_web");
    expect(calls).toHaveLength(1);
    expect(calls[0]).toEqual({ cmd: "delete_preview", args: { serverId: "forge", key: "h_web" } });
    expect(outcome.kind).toBe("ready");
    if (outcome.kind !== "ready") return;
    expect(outcome.preview.folders[0]).toMatchObject({
      path: "/srv/hearth/data/web",
      bytes: 2048,
      fate: "kept_shared",
      sharedWith: ["base"],
      sharedWithUnmanaged: true,
      unmeasured: false,
    });
    expect(outcome.preview.folders[1]).toMatchObject({ bytes: 0, items: 0, unmeasured: true });
    expect(outcome.preview.serverFolders).toEqual(["/mnt/jeux"]);
  });

  it("lit un refus de la consultation par son code", async () => {
    ipc(() => ({ kind: "refused", refusal: { kind: "service_adopted" } }));
    expect(await new TauriLinkBridge().readDeletePreview("forge", "e_old")).toEqual({
      kind: "refused",
      refusal: { kind: "service_adopted" },
    });
  });

  it("supprime : les deux cases, le nom et le mot de passe partent, l'issue acceptée porte l'opération", async () => {
    const calls = ipc(() => ({ kind: "accepted", operationId: "01OP" }));
    const outcome = await new TauriLinkBridge().deleteService(
      "forge",
      "h_web",
      { eraseData: true, eraseBackups: false, confirmName: "web" },
      "mot de passe",
    );
    expect(outcome).toEqual({ kind: "accepted", operationId: "01OP" });
    expect(calls[0]).toEqual({
      cmd: "delete_service",
      args: {
        serverId: "forge",
        key: "h_web",
        deletion: { eraseData: true, eraseBackups: false, confirmName: "web" },
        adminPassword: "mot de passe",
      },
    });
  });

  it("lit un refus par son code et un résultat inconnu sans rien rejouer", async () => {
    const refusal = { kind: "service_busy", by: "claire", action: "backup" };
    ipc(() => ({ kind: "refused", refusal }));
    expect(
      await new TauriLinkBridge().deleteService(
        "forge",
        "h_web",
        { eraseData: false, eraseBackups: false, confirmName: null },
        null,
      ),
    ).toEqual({ kind: "refused", refusal });
    const calls = ipc(() => ({ kind: "unknown", opId: "01KEY" }));
    expect(
      await new TauriLinkBridge().deleteService(
        "forge",
        "h_web",
        { eraseData: false, eraseBackups: false, confirmName: null },
        null,
      ),
    ).toEqual({ kind: "unknown", opId: "01KEY" });
    expect(calls).toHaveLength(1);
  });

  it("une panne de la coquille reste une panne typée, jamais un succès simulé", async () => {
    ipc(() => {
      throw { kind: "missing_services_right" };
    });
    await expect(new TauriLinkBridge().readDeletePreview("forge", "h_web")).rejects.toMatchObject({
      failure: { kind: "missing_services_right" },
    });
    await expect(
      new TauriLinkBridge().deleteService(
        "forge",
        "h_web",
        { eraseData: false, eraseBackups: false, confirmName: null },
        "x",
      ),
    ).rejects.toBeInstanceOf(LinkCommandError);
  });
});
