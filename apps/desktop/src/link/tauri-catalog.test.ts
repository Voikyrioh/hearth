import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Le Catalogue et l'installation depuis une fiche (HRT-102) : le pont RÉEL contre l'IPC simulé de Tauri. Chaque geste est
// UNE commande typée (`read_catalog`, `read_fiche`, `install_fiche`, `open_fiche_link`) ; la coquille Rust lit l'agent
// et juge ses refus par leurs codes (tests de `service_ops/catalog.rs`), le pont ne fait que convertir. Ce fichier ne
// prouve PAS l'appel réel de la coquille (WebView).
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

const detailDto = {
  card: {
    id: "terraria",
    name: "Terraria",
    summary: "Le serveur officiel.",
    version: 2,
    status: "preview",
    installable: true,
  },
  digest: "ab".repeat(32),
  image: { repository: "ryshe/terraria", tag: "vanilla-1.4.5.8", digest: null },
  ownerUid: 0,
  ownerGid: 0,
  variants: [],
  ports: [
    {
      id: "game",
      container: 7777,
      protocol: "tcp",
      role: "game",
      publish: "always",
      defaultHost: 7777,
    },
  ],
  folders: [{ id: "world", role: "world", target: "/worlds", backup: true }],
  agreements: [{ id: "eula", title: "Contrat", url: "https://example.org/eula", sentence: "s" }],
  questions: [
    {
      id: "password",
      label: "Mot de passe",
      help: "h",
      secret: true,
      proposed: "SENTINEL-DEFAULT",
      kind: "secret",
      choices: [],
      min: null,
      max: null,
      minLen: 0,
      maxLen: 100,
      charset: null,
      frozenAfterCreation: false,
      variants: [],
      secretKind: "game_password",
      obtainUrl: null,
    },
    {
      id: "max_players",
      label: "Joueurs",
      help: "h",
      secret: false,
      proposed: "8",
      kind: "int",
      choices: [],
      min: 1,
      max: 255,
      minLen: null,
      maxLen: null,
      charset: null,
      frozenAfterCreation: false,
      variants: [],
      secretKind: null,
      obtainUrl: null,
    },
  ],
  resources: {
    cpu: { minMillis: null, recommendedMillis: null, evidence: { kind: "unmeasured" } },
    memory: {
      minBytes: 2147483648,
      recommendedBytes: 4294967296,
      evidence: { kind: "measured_idle" },
    },
    disk: {
      installBytes: 1073741824,
      evidence: { kind: "publisher", url: "https://example.org/size" },
    },
  },
  firstStart: {
    readyTimeoutSeconds: 1800,
    worldGeneration: true,
    downloads: [
      { what: "fichiers", source: "l'éditeur", bytes: 5, evidence: { kind: "measured" } },
    ],
    stages: ["Chargement"],
  },
  stop: { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: false },
  imposed: [{ label: "Port", value: "7777", why: "Le port du jeu." }],
  themes: [{ id: "game", label: "Ma partie", order: 1 }],
  settings: [
    {
      question: {
        id: "motd",
        label: "Message d'accueil",
        help: "h",
        secret: false,
        proposed: null,
        kind: "text",
        choices: [],
        min: null,
        max: null,
        minLen: 0,
        maxLen: 100,
        charset: "line",
        frozenAfterCreation: false,
        variants: [],
        secretKind: null,
        obtainUrl: null,
      },
      key: "motd",
      theme: "game",
      level: "common",
      gameDefault: "",
      imposedWhy: null,
      since: null,
      until: null,
      replacedBy: null,
      list: null,
    },
    {
      question: {
        id: "password",
        label: "Mot de passe",
        help: "h",
        secret: true,
        proposed: "SENTINEL-DEFAULT",
        kind: "secret",
        choices: [],
        min: null,
        max: null,
        minLen: 0,
        maxLen: 100,
        charset: null,
        frozenAfterCreation: false,
        variants: [],
        secretKind: "game_password",
        obtainUrl: null,
      },
      key: "password",
      theme: "game",
      level: "essential",
      gameDefault: "SENTINEL-GAME-DEFAULT",
      imposedWhy: null,
      since: null,
      until: null,
      replacedBy: null,
      list: null,
    },
  ],
  settingsUnreadable: 2,
  unreadable: 0,
};

const install = {
  name: "mon-terraria",
  fiche: "terraria",
  version: 2,
  digest: "ab".repeat(32),
  variant: null,
  answers: [{ id: "password", value: "SENTINEL-answer" }],
  agreements: ["eula"],
  ports: [{ id: "game", hostPort: 7778 }],
  optionalPorts: [],
  resources: { memoryBytes: 2 * 1024 ** 3, cpuMillis: 1000 },
  accept: { reservedPorts: false, lowDisk: true },
};

describe("le pont réel, catalogue", () => {
  it("lit la liste des fiches avec celles que l'agent a écartées", async () => {
    const calls = ipc(() => ({
      fiches: [detailDto.card],
      refused: [{ dir: "autre", invariant: "I2" }],
      unreadable: 1,
    }));
    const catalog = await new TauriLinkBridge().readCatalog("forge");
    expect(calls).toEqual([{ cmd: "read_catalog", args: { serverId: "forge" } }]);
    expect(catalog).toEqual({
      fiches: [detailDto.card],
      refused: [{ dir: "autre", invariant: "I2" }],
      unreadable: 1,
    });
  });

  it("lit le détail d'une fiche avec ses preuves, son premier démarrage, son arrêt et ce qu'elle impose", async () => {
    const calls = ipc(() => ({ kind: "found", fiche: detailDto }));
    const read = await new TauriLinkBridge().readFiche("forge", "terraria");
    expect(calls[0]).toEqual({ cmd: "read_fiche", args: { serverId: "forge", fiche: "terraria" } });
    expect(read.kind).toBe("found");
    if (read.kind !== "found") return;
    expect(read.fiche.digest).toBe(detailDto.digest);
    expect(read.fiche.resources.memory.evidence).toEqual({ kind: "measured_idle" });
    expect(read.fiche.resources.disk.evidence).toEqual({
      kind: "publisher",
      url: "https://example.org/size",
    });
    expect(read.fiche.firstStart?.readyTimeoutSeconds).toBe(1800);
    expect(read.fiche.stop).toEqual({
      graceSeconds: 10,
      worldSaved: "by_sequence",
      sequenceTested: false,
    });
    expect(read.fiche.imposed).toEqual([{ label: "Port", value: "7777", why: "Le port du jeu." }]);
    // Un secret ne porte jamais de valeur proposée, même si la coquille en rendait une.
    expect(read.fiche.questions[0]?.proposed).toBeNull();
    expect(read.fiche.questions[1]?.proposed).toBe("8");
  });

  it("lit une fiche inconnue comme « inconnue »", async () => {
    ipc(() => ({ kind: "unknown" }));
    expect(await new TauriLinkBridge().readFiche("forge", "nope")).toEqual({ kind: "unknown" });
  });

  it("installe : la demande entière, les réponses et le mot de passe partent, l'issue acceptée porte l'opération", async () => {
    const calls = ipc(() => ({ kind: "accepted", operationId: "01OP", serviceKey: "h_01J" }));
    const outcome = await new TauriLinkBridge().installFiche("forge", install, "mot de passe");
    expect(outcome).toEqual({ kind: "accepted", operationId: "01OP", serviceKey: "h_01J" });
    expect(calls[0]?.cmd).toBe("install_fiche");
    expect(calls[0]?.args).toEqual({
      serverId: "forge",
      installation: install,
      adminPassword: "mot de passe",
    });
    // La demande ne dit ni image, ni variable, ni commande, ni dossier : la fiche les décide.
    const sent = JSON.stringify(calls[0]?.args);
    expect(sent).not.toMatch(/"image"|"env"|"command"|"mounts"/);
  });

  it("lit les refus propres aux fiches par leur code et un résultat inconnu sans rien rejouer", async () => {
    for (const refusal of [
      { kind: "fiche_unknown" },
      { kind: "fiche_changed" },
      { kind: "agreement_required", ids: ["eula"] },
      { kind: "answer_refused", id: "password", reason: "length", character: null },
      { kind: "host_too_small", lack: "memory", needed: 4, has: 2 },
      { kind: "no_space_to_install", neededBytes: 10, freeBytes: 4, missingBytes: 6 },
    ]) {
      ipc(() => ({ kind: "refused", refusal }));
      expect(await new TauriLinkBridge().installFiche("forge", install, null)).toEqual({
        kind: "refused",
        refusal,
      });
    }
    const calls = ipc(() => ({ kind: "unknown", opId: "01KEY" }));
    expect(await new TauriLinkBridge().installFiche("forge", install, null)).toEqual({
      kind: "unknown",
      opId: "01KEY",
    });
    expect(calls).toHaveLength(1);
  });

  it("une panne de la coquille reste une panne typée, jamais un succès simulé", async () => {
    ipc(() => {
      throw { kind: "missing_services_right" };
    });
    await expect(new TauriLinkBridge().installFiche("forge", install, "x")).rejects.toBeInstanceOf(
      LinkCommandError,
    );
  });

  it("demande d'ouvrir une adresse de la fiche par son genre et son identifiant, jamais par l'adresse", async () => {
    const calls = ipc(() => null);
    await new TauriLinkBridge().openFicheLink("forge", "terraria", "agreement", "eula");
    expect(calls[0]).toEqual({
      cmd: "open_fiche_link",
      args: { serverId: "forge", fiche: "terraria", kind: "agreement", key: "eula" },
    });
    expect(JSON.stringify(calls)).not.toContain("https://");
  });
});
