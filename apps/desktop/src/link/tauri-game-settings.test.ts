import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { SaveSettingsRequest } from "./gameSettings";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Les réglages d'un serveur de jeu (HRT-109) : le pont RÉEL contre l'IPC simulé de Tauri. Chaque geste est UNE commande
// typée (`read_game_settings`, `analyze_game_settings`, `save_game_settings`) ; la coquille Rust lit l'agent et juge ses
// refus par leurs codes (tests de `service_ops/game_settings.rs`), le pont ne fait que convertir. Ce fichier ne prouve PAS
// l'appel réel de la coquille (WebView).
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

const value = (id: string, state: string, text: string | null) => ({
  id,
  state,
  value: text,
  frozen: false,
  imposed: false,
  gone: false,
  effect: "restart",
});

const viewDto = {
  rev: 3,
  fiche: { id: "terraria", version: 2, digest: "ab".repeat(32) },
  fileState: "ok",
  fileStamp: "cd".repeat(32),
  fileChanged: true,
  rawEdit: true,
  settings: [
    value("motd", "written", "Salut"),
    // Un agent qui rendrait la valeur d'un secret : le pont la jette.
    value("password", "secret_set", "FUITE"),
  ],
  drifts: [{ id: "motd", hearth: "Salut", file: "Autre", fileValid: true }],
  others: [
    { key: "mod_x", value: "2", masked: false },
    { key: "api_token", value: "FUITE-2", masked: true },
  ],
  imposedBroken: ["upnp"],
};

const request: SaveSettingsRequest = {
  baseRev: 3,
  fileStamp: "cd".repeat(32),
  origin: "form",
  effect: "restart",
  changes: [{ id: "password", action: "set", value: "SENTINEL-secret" }],
  others: [],
  driftChoices: [{ id: "motd", choice: "keep_file" }],
  importText: null,
};

describe("le pont réel, réglages d'un jeu", () => {
  it("lit la vue d'un service et n'en garde jamais un secret", async () => {
    const calls = ipc(() => ({ kind: "found", view: viewDto }));
    const read = await new TauriLinkBridge().readGameSettings("forge", "h_01J");
    expect(calls[0]).toEqual({
      cmd: "read_game_settings",
      args: { serverId: "forge", key: "h_01J" },
    });
    if (read.kind !== "found") throw new Error("des réglages étaient attendus");
    expect(read.view.rev).toBe(3);
    expect(read.view.fileChanged).toBe(true);
    expect(read.view.settings[1]).toMatchObject({
      id: "password",
      state: "secret_set",
      value: null,
    });
    expect(read.view.others[1]).toEqual({ key: "api_token", value: null, masked: true });
    expect(read.view.drifts[0]).toEqual({
      id: "motd",
      hearth: "Salut",
      file: "Autre",
      fileValid: true,
    });
    expect(JSON.stringify(read)).not.toContain("FUITE");
  });

  it("lit « pas de réglages » d'un service ordinaire comme tel, jamais une panne", async () => {
    ipc(() => ({ kind: "absent" }));
    expect(await new TauriLinkBridge().readGameSettings("forge", "c_web")).toEqual({
      kind: "absent",
    });
  });

  it("envoie le texte collé tel quel à la lecture et lit les lignes sans les secrets", async () => {
    const calls = ipc(() => ({
      kind: "read",
      view: {
        lines: [
          { id: "motd", state: "take", reason: null, pasted: "Salut", secret: false },
          { id: "password", state: "take", reason: null, pasted: "FUITE", secret: true },
        ],
        unknown: 1,
        duplicates: 0,
        imposedUndone: ["port"],
      },
    }));
    const outcome = await new TauriLinkBridge().analyzeGameSettings("forge", "h_01J", "motd=Salut");
    expect(calls[0]?.args).toEqual({ serverId: "forge", key: "h_01J", text: "motd=Salut" });
    if (outcome.kind !== "read") throw new Error("lu attendu");
    expect(outcome.analysis.lines[1]).toMatchObject({ secret: true, pasted: null });
    expect(outcome.analysis.imposedUndone).toEqual(["port"]);
    expect(JSON.stringify(outcome)).not.toContain("FUITE");
  });

  it("lit un texte illisible comme tel, avec sa ligne", async () => {
    ipc(() => ({ kind: "unreadable", reason: "no_separator", line: 4 }));
    expect(await new TauriLinkBridge().analyzeGameSettings("forge", "h_01J", "x")).toEqual({
      kind: "unreadable",
      reason: "no_separator",
      line: 4,
    });
  });

  it("enregistre : la demande entière et le mot de passe de confirmation partent, l'issue porte l'effet", async () => {
    const calls = ipc(() => ({
      kind: "saved",
      rev: 4,
      effect: "restart",
      fileWritten: true,
      changed: 1,
      adopted: ["motd"],
    }));
    const outcome = await new TauriLinkBridge().saveGameSettings(
      "forge",
      "h_01J",
      request,
      "mot de passe",
    );
    expect(calls[0]?.cmd).toBe("save_game_settings");
    expect(calls[0]?.args).toEqual({
      serverId: "forge",
      key: "h_01J",
      settings: request,
      adminPassword: "mot de passe",
    });
    expect(outcome).toEqual({
      kind: "saved",
      rev: 4,
      effect: "restart",
      fileWritten: true,
      changed: 1,
      adopted: ["motd"],
    });
    // L'empreinte du fichier est toujours envoyée.
    expect(JSON.stringify(calls[0]?.args)).toContain('"fileStamp"');
  });

  it("lit les refus par leur code et un résultat inconnu sans rien rejouer", async () => {
    for (const refusal of [
      { kind: "wrong_password" },
      { kind: "conflict", by: "marie", effectMismatch: false },
      { kind: "file_changed", ids: ["motd"] },
      { kind: "file_unreadable", reason: "unclosed_tag", line: 12 },
      { kind: "setting_refused", id: "motd", reason: "length", character: null },
      { kind: "recreate_not_available", ids: ["java_args"] },
      { kind: "operation_in_progress" },
    ]) {
      ipc(() => ({ kind: "refused", refusal }));
      expect(await new TauriLinkBridge().saveGameSettings("forge", "h_01J", request, null)).toEqual(
        { kind: "refused", refusal },
      );
    }
    const calls = ipc(() => ({ kind: "unknown", opId: "01KEY" }));
    expect(await new TauriLinkBridge().saveGameSettings("forge", "h_01J", request, null)).toEqual({
      kind: "unknown",
      opId: "01KEY",
    });
    expect(calls).toHaveLength(1);
  });

  it("une panne de la coquille reste une panne typée, jamais un succès simulé", async () => {
    ipc(() => {
      throw { kind: "missing_services_right" };
    });
    await expect(
      new TauriLinkBridge().saveGameSettings("forge", "h_01J", request, "x"),
    ).rejects.toBeInstanceOf(LinkCommandError);
  });
});
