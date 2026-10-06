import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AuditEntryDto } from "@/bindings";
import { EMPTY_AUDIT_FILTER, toAuditEntry, toAuditFilterDto, toAuditPage } from "./audit";
import { LINK_EVENTS, TauriLinkBridge } from "./tauri";
import { LinkCommandError } from "./types";

// HRT-14 : le pont réel du journal. Une commande typée par lecture ; le filtre est typé ; aucun
// chemin, aucune méthode, aucune adresse ne part de l'interface.

const dto: AuditEntryDto = {
  id: 42,
  at: "2026-10-04T10:30:15.250Z",
  account: "marie",
  origin: { kind: "client", name: "poste", addr: "10.0.0.7", text: "10.0.0.7 (poste)" },
  action: "account.create",
  actionLabel: "Création de compte",
  target: "paul",
  outcome: "ok",
  reason: null,
  repeatCount: 0,
};

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

describe("conversions du journal", () => {
  it("garde la date source UTC et lit la date en millisecondes ; une date illisible garde l'entrée", () => {
    const entry = toAuditEntry(dto);
    expect(entry?.at).toBe("2026-10-04T10:30:15.250Z");
    expect(entry?.atMs).toBe(Date.UTC(2026, 9, 4, 10, 30, 15, 250));
    const odd = toAuditEntry({ ...dto, at: "pas une date" });
    expect(odd?.id).toBe(42);
    expect(Number.isNaN(odd?.atMs)).toBe(true);
  });

  it("écarte une entrée sans identifiant valide, sans casser la page", () => {
    const page = toAuditPage({
      events: [dto, { ...dto, id: null }, { ...dto, id: 0 }, { ...dto, id: 41 }],
      nextBefore: 41,
    });
    expect(page.events.map((e) => e.id)).toEqual([42, 41]);
    expect(page.nextBefore).toBe(41);
  });

  it("le filtre part typé : texte vide devient absent, rien d'autre qu'un filtre", () => {
    expect(toAuditFilterDto(EMPTY_AUDIT_FILTER)).toEqual({
      accounts: [],
      kinds: [],
      outcomes: [],
      fromS: null,
      toS: null,
      text: null,
    });
  });
});

describe("TauriLinkBridge : journal", () => {
  it("readAudit appelle SA commande avec le filtre et le curseur, et convertit la page", async () => {
    const calls = ipc((cmd) =>
      cmd === "read_audit" ? { events: [dto], nextBefore: null } : undefined,
    );
    const bridge = new TauriLinkBridge();
    const filter = { ...EMPTY_AUDIT_FILTER, accounts: ["marie"], text: "10.0.0.7" };
    const page = await bridge.readAudit("srv", filter, 99);
    expect(page.events[0]?.account).toBe("marie");
    expect(calls).toEqual([
      {
        cmd: "read_audit",
        args: {
          serverId: "srv",
          filter: { ...toAuditFilterDto(filter) },
          before: 99,
        },
      },
    ]);
    // Aucun paramètre libre : seulement l'identifiant du serveur, le filtre typé et le curseur.
    expect(Object.keys(calls[0]?.args ?? {}).sort()).toEqual(["before", "filter", "serverId"]);
  });

  it("un refus du serveur (lecture seule) devient un échec typé « forbidden »", async () => {
    ipc((cmd) => {
      if (cmd === "read_audit") throw { kind: "forbidden" };
      return undefined;
    });
    const bridge = new TauriLinkBridge();
    await expect(bridge.readAudit("srv", EMPTY_AUDIT_FILTER, null)).rejects.toMatchObject({
      failure: { kind: "forbidden" },
    });
    await expect(bridge.readAudit("srv", EMPTY_AUDIT_FILTER, null)).rejects.toBeInstanceOf(
      LinkCommandError,
    );
  });

  it("exportAudit ne passe ni chemin ni nom de fichier : le fichier est choisi dans la boîte du système", async () => {
    const calls = ipc((cmd) =>
      cmd === "export_audit" ? { saved: true, truncated: false } : undefined,
    );
    const bridge = new TauriLinkBridge();
    const result = await bridge.exportAudit("srv", EMPTY_AUDIT_FILTER);
    expect(result).toEqual({ saved: true, truncated: false });
    expect(Object.keys(calls[0]?.args ?? {}).sort()).toEqual(["filter", "serverId"]);
  });

  it("onAudit ne livre que les entrées de SON serveur, converties ; se désabonne", async () => {
    ipc(() => undefined);
    const bridge = new TauriLinkBridge();
    const seen: number[] = [];
    const off = await bridge.onAudit("srv", (entry) => seen.push(entry.id));
    await emit(LINK_EVENTS.audit, { serverId: "srv", event: dto });
    await emit(LINK_EVENTS.audit, { serverId: "autre", event: { ...dto, id: 43 } });
    await emit(LINK_EVENTS.audit, { serverId: "srv", event: { ...dto, id: null } });
    await vi.waitFor(() => expect(seen).toEqual([42]));
    off();
    await emit(LINK_EVENTS.audit, { serverId: "srv", event: { ...dto, id: 44 } });
    expect(seen).toEqual([42]);
  });
});
