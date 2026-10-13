import { readFileSync } from "node:fs";
import { join } from "node:path";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { AccountListDto, ServiceRightsDto } from "@/bindings";
import { setLinkBridge } from "@/link";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { failureMessage } from "./messages";
import { TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// Le pont RÉEL contre la fiche de référence de la coquille : `src-tauri/tests/fixtures/rights.json` est exactement
// ce que la coquille Rust écrit vers l'interface (elle la compare, champ par champ, à ses types : tests/rights_wire.rs ;
// la lecture réelle contre un vrai agent à la même forme : tests/accounts_runtime.rs). Ici la même fiche traverse l'IPC
// simulé de Tauri, le pont réel et le magasin : le dernier maillon de la chaîne que ce poste peut prouver. Non prouvé
// ici : l'IPC réel de la WebView (`invoke`) dans l'application construite.
const sheet = JSON.parse(
  readFileSync(join(process.cwd(), "src-tauri", "tests", "fixtures", "rights.json"), "utf8"),
) as {
  serviceRights: Record<string, ServiceRightsDto>;
  accountList: AccountListDto;
  failure: { kind: string };
};

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

beforeEach(() => setActivePinia(createPinia()));
afterEach(() => {
  clearMocks();
  setLinkBridge(null);
});

describe("le droit de gérer les services par le pont réel", () => {
  it("UNE commande de lecture, sans paramètre libre, qui rend la forme de la fiche", async () => {
    const calls = ipc(() => sheet.serviceRights.adminGranted);
    const rights = await new TauriLinkBridge().readServiceRights("forge");
    expect(calls).toEqual([{ cmd: "read_service_rights", args: { serverId: "forge" } }]);
    expect(rights).toEqual(sheet.serviceRights.adminGranted);
  });

  it("la liste des comptes porte le droit de chacun et la capacité de l'agent", async () => {
    ipc(() => sheet.accountList);
    const list = await new TauriLinkBridge().listAccounts("forge");
    expect(list.rightsKnown).toBe(true);
    expect(list.accounts.map((a) => [a.username, a.managesServices])).toEqual([
      ["marie", true],
      ["paul", false],
    ]);
  });

  it("un refus faute du droit devient l'échec typé de la fiche, et son texte ne dit jamais « administrateur »", async () => {
    ipc((cmd) => {
      if (cmd === "read_service_rights") throw sheet.failure;
      return null;
    });
    const error = await new TauriLinkBridge().readServiceRights("forge").then(
      () => null,
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(LinkCommandError);
    const failure = (error as LinkCommandError).failure;
    expect(failure).toEqual({ kind: "missing_services_right" });
    const text = failureMessage(failure);
    expect(text).toBe("Tu n'as pas le droit de gérer les services de ce serveur.");
    expect(text).not.toMatch(/administrateur|rôle/i);
  });

  it("de la coquille au magasin : chaque profil de la fiche arrive jusqu'au droit lu par l'écran", async () => {
    for (const [name, dto] of Object.entries(sheet.serviceRights)) {
      setActivePinia(createPinia());
      setLinkBridge(new TauriLinkBridge());
      ipc(() => dto);
      const store = useServiceRightsStore();
      await store.load("forge");
      expect(store.of("forge"), name).toEqual({ phase: "known", rights: dto });
      clearMocks();
    }
  });

  it("un échec de lecture du droit n'invente pas de droit", async () => {
    setLinkBridge(new TauriLinkBridge());
    ipc((cmd) => {
      if (cmd === "read_service_rights") throw { kind: "unreachable" };
      return null;
    });
    const store = useServiceRightsStore();
    await store.load("forge");
    expect(store.of("forge")).toEqual({ phase: "failed", rights: null });
  });
});
