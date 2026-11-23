import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";

// Les mods d'un serveur Minecraft (HRT-260) : le pont RÉEL contre l'IPC simulé de Tauri. Chaque geste est UNE commande
// typée ; la coquille Rust lit les réponses de l'agent par leur code (tests de `service_ops/mods.rs`), le pont ne fait
// que convertir : un mot du fil inconnu se lit « inconnu », jamais une permission.
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

const identity = {
  id: "lithium",
  name: "Lithium",
  version: "0.11.2",
  loader: "fabric",
  minecraftDeclared: true,
  environment: "server",
};
const verdict = { code: "compatible", missing: [], neededBy: [] };

describe("le pont réel, mods", () => {
  it("lit la liste : la clé seule part, un mot du fil inconnu devient « inconnu »", async () => {
    const calls = ipc(() => ({
      kind: "ready",
      view: {
        applicable: "something_new",
        game: {
          loader: "fabric",
          minecraft: "1.20.1",
          minecraftFrom: "log",
          updatesEveryStart: false,
          pack: "none",
        },
        mods: [
          {
            file: "lithium.jar",
            rank: 0,
            place: "active",
            enabled: true,
            origin: "elsewhere",
            readableName: true,
            identity,
            verdict: { ...verdict, code: "from_the_future" },
          },
        ],
        unread: 3,
        truncated: false,
      },
    }));
    const outcome = await new TauriLinkBridge().readMods("forge", "h_mc");
    expect(calls).toEqual([{ cmd: "list_mods", args: { serverId: "forge", key: "h_mc" } }]);
    if (outcome.kind !== "ready") throw new Error("liste attendue");
    expect(outcome.view.applicable).toBe("unknown");
    expect(outcome.view.mods[0]).toMatchObject({
      origin: "unknown",
      verdict: { code: "unknown" },
      identity: { environment: "server" },
    });
    expect(outcome.view.unread).toBe(3);
  });

  it("contrôle des dépôts : les identifiants seuls partent", async () => {
    const calls = ipc(() => ({
      kind: "ready",
      view: {
        files: [
          {
            deposit: "D1",
            outcome: "refused",
            identity,
            targetFile: null,
            replaces: null,
            verdict: null,
            refusal: "MOD_INCOMPATIBLE_LOADER",
            refusalValues: { loader: "forge", serverLoader: "fabric", serverMinecraft: "1.20.1" },
          },
        ],
      },
    }));
    const outcome = await new TauriLinkBridge().checkMods("forge", "h_mc", ["D1"]);
    expect(calls[0]).toEqual({
      cmd: "check_mods",
      args: { serverId: "forge", key: "h_mc", deposits: ["D1"] },
    });
    expect(outcome).toMatchObject({
      kind: "ready",
      files: [{ outcome: "refused", refusal: "MOD_INCOMPATIBLE_LOADER" }],
    });
  });

  it("applique un lot : le lot et le mot de passe partent, l'issue acceptée porte l'opération", async () => {
    const calls = ipc(() => ({ kind: "accepted", operationId: "01OP", backupId: "01BK" }));
    const batch = {
      add: [{ deposit: "D1", sha256: "ab", replace: null }],
      retire: ["a.jar"],
      restore: [],
      disable: ["b.jar"],
      enable: [],
    };
    const outcome = await new TauriLinkBridge().applyMods("forge", "h_mc", batch, "secret");
    expect(calls[0]).toEqual({
      cmd: "apply_mods",
      args: { serverId: "forge", key: "h_mc", batch, adminPassword: "secret" },
    });
    expect(outcome).toEqual({ kind: "accepted", operationId: "01OP", backupId: "01BK" });
  });

  it("un refus propre aux mods et un résultat inconnu arrivent tels quels", async () => {
    ipc(() => ({
      kind: "refused",
      refusal: { kind: "mod", code: "MOD_IN_MODRINTH_PACK", deposit: null, reason: null },
    }));
    const refused = await new TauriLinkBridge().applyMods("forge", "h_mc", emptyBatch(), null);
    expect(refused).toMatchObject({ kind: "refused", refusal: { code: "MOD_IN_MODRINTH_PACK" } });
    clearMocks();
    ipc(() => ({ kind: "unknown", opId: "x" }));
    expect(await new TauriLinkBridge().applyMods("forge", "h_mc", emptyBatch(), null)).toEqual({
      kind: "unknown",
      opId: "x",
    });
  });

  it("lit la liste des mods d'une sauvegarde avec sa comparaison, et le journal d'une opération", async () => {
    const calls = ipc((cmd) =>
      cmd === "read_backup_mods"
        ? {
            kind: "ready",
            view: {
              state: "recorded",
              recordedAt: "2026-11-17T20:00:00Z",
              game: { loader: "fabric", minecraft: "1.20.1" },
              counts: { active: 2, disabled: 1, retired: 0, notListed: 0 },
              mods: [
                { file: "a.jar", rank: 0, place: "disabled", origin: "hearth", identity },
                { file: null, rank: 1, place: "weird", origin: "manual", identity: null },
              ],
              compare: {
                complete: true,
                unread: 0,
                same: false,
                changes: [
                  {
                    kind: "version_changed",
                    id: "lithium",
                    name: "Lithium",
                    file: null,
                    backupVersion: "0.11.1",
                    currentVersion: "0.11.2",
                  },
                ],
              },
            },
          }
        : {
            kind: "ready",
            tail: { lines: ["a"], reason: "duplicate_mod", sentence: "Un mod est en double." },
          },
    );
    const bridge = new TauriLinkBridge();
    const backup = await bridge.readBackupMods("forge", "h_mc", "01BK", true);
    expect(calls[0]).toEqual({
      cmd: "read_backup_mods",
      args: { serverId: "forge", key: "h_mc", backupId: "01BK", compare: true },
    });
    if (backup.kind !== "ready") throw new Error("liste attendue");
    expect(backup.view.mods.map((mod) => mod.place)).toEqual(["disabled", "unknown"]);
    expect(backup.view.compare?.changes[0]).toMatchObject({ kind: "version_changed" });
    const tail = await bridge.readOperationTail("forge", "01OP");
    expect(calls[1]).toEqual({
      cmd: "read_operation_tail",
      args: { serverId: "forge", operationId: "01OP" },
    });
    expect(tail).toMatchObject({ kind: "ready", tail: { reason: "duplicate_mod" } });
  });
});

function emptyBatch() {
  return { add: [], retire: ["a.jar"], restore: [], disable: [], enable: [] };
}
