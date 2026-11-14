import { describe, expect, it } from "vitest";
import type { DeletePreview, FolderPreview, ServiceInfo, ServiceOperation } from "@/link";
import {
  cancelRefusalText,
  deleteDoor,
  endOf,
  erasableFolders,
  failureCauseText,
  folderNotes,
  folderRows,
  mustRetype,
  nameMatches,
  offersBackupsBox,
  offersDataBox,
  previewRefusalText,
  refusalText,
  requestOf,
  staleNotice,
  stepsOf,
  unmeasuredNotes,
} from "./delete";
import type { RightsGate } from "./rights";

// Les règles pures de la suppression (HRT-86, BR-DEL-*) : la porte du bouton, les dossiers et leur sort, la demande,
// les étapes et l'issue. L'agent reste l'arbitre ; ces règles disent d'avance, honnêtement, ce qu'il jugera.
const allowed: RightsGate = { kind: "allowed" };
const free = { locked: false, by: null, own: false };

function service(patch: Partial<ServiceInfo> = {}): ServiceInfo {
  return {
    key: "h_web",
    name: "web",
    origin: "hearth",
    state: "running",
    adopted: null,
    ...patch,
  } as ServiceInfo;
}

function door(patch: Partial<Parameters<typeof deleteDoor>[0]> = {}) {
  return deleteDoor({
    service: service(),
    gone: false,
    supported: true,
    gate: allowed,
    lock: free,
    engineReady: true,
    ...patch,
  });
}

const folder = (patch: Partial<FolderPreview> = {}): FolderPreview => ({
  path: "/srv/hearth/data/web",
  bytes: 1024,
  items: 3,
  partial: false,
  unmeasured: false,
  fate: "erasable",
  sharedWith: [],
  sharedWithUnmanaged: false,
  ...patch,
});

const preview = (patch: Partial<DeletePreview> = {}): DeletePreview => ({
  name: "web",
  image: "nginx:1.27",
  running: true,
  stopGraceS: 10,
  containerMissing: false,
  folders: [folder()],
  serverFolders: [],
  volumes: 0,
  measuredInTime: true,
  backups: 0,
  ...patch,
});

describe("la porte du bouton (BR-DEL-002, 003, 026)", () => {
  it("est ouverte pour un service créé par Hearth dans un état supprimable", () => {
    for (const state of ["running", "stopped", "error", "restart_loop"] as const) {
      expect(door({ service: service({ state }) }), state).toEqual({ kind: "ready" });
    }
  });

  it("n'existe pas : agent d'avant, service disparu, déploiement en cours, conteneur jamais confié", () => {
    expect(door({ supported: false })).toEqual({ kind: "hidden" });
    expect(door({ gone: true })).toEqual({ kind: "hidden" });
    expect(door({ service: service({ state: "deploying" }) })).toEqual({ kind: "hidden" });
    expect(door({ service: service({ origin: "external" }) })).toEqual({ kind: "hidden" });
  });

  it("un service adopté ne propose que la phrase", () => {
    const adopted = service({ origin: "external", adopted: { by: "claire", at: 0 } });
    expect(door({ service: adopted })).toEqual({ kind: "adopted" });
  });

  it("le droit manquant bloque d'abord, avec sa propre raison", () => {
    const denied: RightsGate = { kind: "denied", isAdmin: true, command: null };
    expect(door({ gate: denied, lock: { locked: true, by: "x", own: false } })).toMatchObject({
      kind: "blocked",
      rights: true,
    });
  });

  it("une action en cours, une transition, un état non piloté et le moteur bloquent avec leur raison", () => {
    expect(door({ lock: { locked: true, by: "Lancée par claire.", own: false } })).toMatchObject({
      kind: "blocked",
      reason: "Une action est déjà en cours sur ce service.",
      by: "Lancée par claire.",
    });
    for (const state of ["starting", "stopping", "updating"] as const) {
      expect(door({ service: service({ state }) }), state).toMatchObject({
        kind: "blocked",
        reason: "Une action est déjà en cours sur ce service.",
      });
    }
    for (const state of ["paused", "unknown"] as const) {
      expect(door({ service: service({ state }) }), state).toMatchObject({
        kind: "blocked",
        reason: "Hearth ne pilote pas ce service dans son état actuel.",
      });
    }
    expect(door({ engineReady: false })).toMatchObject({ kind: "blocked", rights: false });
  });
});

describe("les dossiers et leur sort (BR-DEL-006 à 009)", () => {
  it("sans case, tout est conservé ; case cochée, le dossier géré libre est effacé", () => {
    const rows = (eraseData: boolean) => folderRows(preview(), eraseData);
    expect(rows(false)[0]).toMatchObject({ fate: "Conservé", erased: false });
    expect(rows(true)[0]).toMatchObject({ fate: "Effacé", erased: true });
  });

  it("un dossier partagé, non vérifiable ou hors de la racine n'est jamais effacé", () => {
    const shown = preview({
      folders: [
        folder({ fate: "kept_shared", sharedWith: ["base"] }),
        folder({ path: "/a", fate: "kept_unverifiable" }),
        folder({ path: "/b", fate: "kept_outside_root" }),
        folder({ path: "/c", fate: "unknown" }),
      ],
    });
    const rows = folderRows(shown, true);
    expect(rows.map((row) => row.erased)).toEqual([false, false, false, false]);
    expect(rows[0]?.fate).toBe("Conservé (partagé)");
    expect(erasableFolders(shown)).toEqual([]);
    expect(offersDataBox(shown)).toBe(false);
    expect(folderNotes(shown).map((note) => note.key)).toEqual([
      "shared-0",
      "unverifiable-1",
      "outside-2",
    ]);
  });

  it("un dossier du serveur est listé, conservé, jamais mesuré", () => {
    const rows = folderRows(preview({ folders: [], serverFolders: ["/mnt/jeux"] }), true);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      path: "/mnt/jeux",
      fate: "Conservé (appartient au serveur)",
      size: "Non mesurée",
      erased: false,
    });
  });

  it("la mesure interrompue dit « plus de » et la note de taille, sinon rien", () => {
    const partial = preview({
      measuredInTime: false,
      folders: [folder({ partial: true, bytes: 2 * 1024 ** 3, items: 5000 })],
    });
    expect(folderRows(partial, false)[0]?.size).toBe("plus de 2 Go (mesure interrompue)");
    expect(unmeasuredNotes(partial).map((note) => note.text)).toEqual([
      "Taille non mesurée pour /srv/hearth/data/web : plus grande que 2 Go.",
    ]);
    expect(unmeasuredNotes(preview())).toEqual([]);
    expect(folderRows(preview({ folders: [folder({ unmeasured: true })] }), false)[0]?.size).toBe(
      "Non mesurée",
    );
  });

  it("un chemin venu du serveur est rendu inoffensif", () => {
    const rows = folderRows(preview({ folders: [folder({ path: "/a‮b" })] }), false);
    expect(rows[0]?.path).not.toContain("‮");
  });

  it("la case des sauvegardes n'est proposée que si Hearth en connaît (BR-DEL-005)", () => {
    expect(offersBackupsBox(preview({ backups: 0 }))).toBe(false);
    expect(offersBackupsBox(preview({ backups: 2 }))).toBe(true);
  });
});

describe("la confirmation (BR-DEL-011)", () => {
  it("le nom retapé est exigé dès qu'une case est cochée, casse comprise", () => {
    expect(mustRetype({ eraseData: false, eraseBackups: false })).toBe(false);
    expect(mustRetype({ eraseData: true, eraseBackups: false })).toBe(true);
    expect(mustRetype({ eraseData: false, eraseBackups: true })).toBe(true);
    expect(nameMatches("web", "web")).toBe(true);
    expect(nameMatches("Web", "web")).toBe(false);
    expect(nameMatches("web ", "web")).toBe(false);
  });

  it("la demande n'envoie le nom que si une case est cochée, jamais par défaut", () => {
    expect(requestOf({ eraseData: false, eraseBackups: false }, "web")).toEqual({
      eraseData: false,
      eraseBackups: false,
      confirmName: null,
    });
    expect(requestOf({ eraseData: true, eraseBackups: false }, "web")).toEqual({
      eraseData: true,
      eraseBackups: false,
      confirmName: "web",
    });
  });

  it("le service qui change sous la fenêtre la rend inactive", () => {
    expect(staleNotice(service(), false, true)).toBeNull();
    expect(staleNotice(null, false, true)).toBe("Ce service n'existe plus.");
    expect(staleNotice(service({ state: "paused" }), false, true)).toContain("En pause");
    expect(staleNotice(service(), true, true)).toBe("Une action est déjà en cours sur ce service.");
    expect(staleNotice(service(), false, false)).toContain("moteur de conteneurs");
  });

  it("chaque refus de l'agent a sa phrase, un code inconnu devient le refus générique", () => {
    expect(refusalText({ kind: "name_mismatch" })).toBe("Le nom saisi ne correspond pas.");
    expect(refusalText({ kind: "service_adopted" })).toContain(
      "ne supprime pas les services créés hors de Hearth",
    );
    expect(refusalText({ kind: "service_not_found" })).toBe("Ce service n'existe plus.");
    expect(refusalText({ kind: "not_drivable" })).toBe(
      "Hearth ne pilote pas ce service dans son état actuel.",
    );
    expect(refusalText({ kind: "from_the_future" })).toBe(
      "Le serveur a refusé la suppression. Le service n'a pas été supprimé.",
    );
    expect(
      refusalText({ kind: "engine_unavailable", engine: "stopped" } as { kind: string }),
    ).toContain("Le service n'a pas été supprimé.");
    expect(previewRefusalText({ kind: "service_adopted" })).toContain("ne supprime pas");
    expect(cancelRefusalText({ kind: "too_late" })).toContain("trop tard");
  });
});

function op(patch: Partial<ServiceOperation> = {}): ServiceOperation {
  return {
    id: "01OP",
    key: "h_web",
    kind: "delete",
    status: "running",
    step: null,
    steps: [],
    percent: null,
    itemsDone: null,
    bytesDone: null,
    bytesTotal: null,
    etaS: null,
    cancellable: true,
    decision: null,
    cause: null,
    ...patch,
  };
}

describe("les étapes et l'issue (BR-DEL-013 à 016)", () => {
  it("l'arrêt n'est une étape que si le service tournait, l'effacement que sur la case", () => {
    const keys = (plan: { stop: boolean; erase: boolean }) =>
      stepsOf(null, plan).map((step) => step.key);
    expect(keys({ stop: false, erase: false })).toEqual(["remove"]);
    expect(keys({ stop: true, erase: false })).toEqual(["stop", "remove"]);
    expect(keys({ stop: true, erase: true })).toEqual(["stop", "remove", "erase"]);
  });

  it("l'étape courante suit le plan de l'agent, forget et remove_container sont une seule ligne", () => {
    const states = (step: string) =>
      stepsOf(op({ step }), { stop: true, erase: true }).map((item) => item.state);
    expect(states("stop")).toEqual(["now", "later", "later"]);
    expect(states("remove_container")).toEqual(["done", "now", "later"]);
    expect(states("forget")).toEqual(["done", "now", "later"]);
    expect(states("erase")).toEqual(["done", "done", "now"]);
  });

  it("l'effacement dit les éléments effacés", () => {
    const erase = stepsOf(op({ step: "erase", itemsDone: 1500 }), {
      stop: false,
      erase: true,
    }).at(-1);
    expect(erase?.bar?.text).toMatch(/1\s500 éléments effacés/);
  });

  it("chaque issue a sa lecture", () => {
    expect(endOf(op({ status: "succeeded" }))).toEqual({ kind: "success", forced: false });
    expect(endOf(op({ status: "succeeded", cause: "forced_stop" }))).toEqual({
      kind: "success",
      forced: true,
    });
    expect(endOf(op({ status: "succeeded", cause: "data_not_erased" }))).toMatchObject({
      kind: "partial",
    });
    expect(endOf(op({ status: "failed", cause: "engine_refused" }))).toEqual({ kind: "failed" });
    expect(endOf(op({ status: "interrupted" }))).toEqual({ kind: "interrupted" });
    expect(endOf(op({ status: "cancelled", steps: [{ step: "stop", state: "done" }] }))).toEqual({
      kind: "cancelled",
      stopped: true,
    });
    expect(endOf(op({ status: "cancelled" }))).toEqual({ kind: "cancelled", stopped: false });
  });

  it("la cause d'un échec se dit par son code, jamais par un texte de l'agent", () => {
    expect(failureCauseText("engine_unavailable")).toBe(
      "Le moteur de conteneurs n'est pas accessible.",
    );
    expect(failureCauseText("engine_refused")).toContain("a refusé");
    expect(failureCauseText("timeout")).toContain("pas répondu à temps");
    expect(failureCauseText("step_failed")).toBeNull();
    expect(failureCauseText(null)).toBeNull();
  });
});
