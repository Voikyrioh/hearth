import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emptyDraft } from "@/audit/filters";
import { type AuditEntry, setLinkBridge } from "@/link";
import { startedApp } from "@/test/app";
import {
  AUDIT_LIVE_DEBOUNCE_MS,
  AUDIT_LIVE_MAX_WAIT_MS,
  AUDIT_PENDING_MAX,
  AUDIT_VERIFY_EVERY_MS,
  AUDIT_WINDOW_MAX,
  mergeDesc,
  useAuditStore,
} from "./audit";
import { useToastsStore } from "./toasts";

// HRT-14, côté store : le journal est une suite contiguë, sans doublon ni trou, dans un ordre
// stable ; un filtre actif vaut aussi pour le direct ; après une coupure on rattrape tout.

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

async function opened(seed = 250) {
  const ctx = await startedApp();
  ctx.bridge.audit.seed("forge", seed);
  const audit = useAuditStore();
  await audit.open("forge");
  await flushPromises();
  return { ...ctx, audit };
}

function ids(list: readonly AuditEntry[]): number[] {
  return list.map((entry) => entry.id);
}

function strictlyDescendingWithoutDuplicates(list: readonly AuditEntry[]): boolean {
  return list.every((entry, i) => i === 0 || (list[i - 1] as AuditEntry).id > entry.id);
}

describe("chargement et pagination", () => {
  it("charge les 100 plus récentes, puis la suite par curseur, dans un ordre stable et sans doublon", async () => {
    const { audit, bridge } = await opened(250);
    expect(audit.status).toBe("ready");
    expect(audit.entries).toHaveLength(100);
    expect(audit.hasMoreBelow).toBe(true);
    const newest = audit.entries[0]?.id;
    await audit.loadMore();
    await audit.loadMore();
    expect(audit.entries).toHaveLength(250);
    expect(audit.hasMoreBelow).toBe(false);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(audit.entries[0]?.id).toBe(newest);
    // Une page n'est jamais demandée deux fois : le curseur avance.
    const cursors = bridge.audit.reads.map((read) => read.before);
    expect(cursors[0]).toBeNull();
    expect(new Set(cursors).size).toBe(cursors.length);
  });

  it("un journal vide s'affiche comme tel, sans erreur", async () => {
    const { audit } = await opened(0);
    expect(audit.status).toBe("ready");
    expect(audit.entries).toHaveLength(0);
  });

  it("un compte lecture seule est refusé : échec typé, rien affiché (BR-AUDIT-001)", async () => {
    const ctx = await startedApp();
    const audit = useAuditStore();
    await audit.open("salon");
    await flushPromises();
    expect(audit.status).toBe("error");
    expect(audit.failure).toEqual({ kind: "forbidden" });
    expect(audit.entries).toHaveLength(0);
    expect(ctx.bridge.audit.reads).toHaveLength(1);
  });

  it("la mémoire est bornée : la fenêtre ne dépasse jamais AUDIT_WINDOW_MAX et dit qu'elle est pleine", async () => {
    const { audit } = await opened(AUDIT_WINDOW_MAX + 400);
    for (let i = 0; i < 40 && !audit.windowFull; i += 1) await audit.loadMore();
    expect(audit.entries).toHaveLength(AUDIT_WINDOW_MAX);
    expect(audit.windowFull).toBe(true);
    expect(audit.hasMoreBelow).toBe(true);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    await audit.loadMore();
    expect(audit.entries).toHaveLength(AUDIT_WINDOW_MAX);
  });

  it("en haut, des entrées en direct au-delà de la fenêtre coupent le BAS, et la suite se relit depuis la dernière gardée", async () => {
    const { audit, bridge } = await opened(AUDIT_WINDOW_MAX);
    for (let i = 0; i < 40 && !audit.windowFull && audit.hasMoreBelow; i += 1) {
      await audit.loadMore();
    }
    expect(audit.entries).toHaveLength(AUDIT_WINDOW_MAX);
    for (let i = 0; i < 5; i += 1) bridge.audit.add("forge");
    expect(audit.entries).toHaveLength(AUDIT_WINDOW_MAX);
    expect(audit.hasMoreBelow).toBe(true);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(audit.nextBefore).toBe(audit.entries[AUDIT_WINDOW_MAX - 1]?.id);
  });
});

describe("direct (BR-AUDIT-010)", () => {
  it("en haut de la liste : l'entrée apparaît tout de suite en tête, une seule fois", async () => {
    const { audit, bridge } = await opened(120);
    const fresh = bridge.audit.add("forge", { account: "léa" });
    expect(audit.entries[0]?.id).toBe(fresh.id);
    expect(audit.newCount).toBe(0);
    expect(audit.knownAccounts).toContain("léa");
    expect(audit.announcement).toBe("1 nouvelle entrée");
    // Le même événement livré deux fois ne double rien.
    bridge.audit.add("forge");
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(audit.entries).toHaveLength(102);
  });

  it("défilé vers le bas : la liste ne bouge pas, le bouton compte « N nouvelles entrées », un clic les montre", async () => {
    const { audit, bridge } = await opened(120);
    audit.setAtTop(false);
    const before = ids(audit.entries);
    bridge.audit.add("forge");
    bridge.audit.add("forge");
    bridge.audit.add("forge");
    expect(ids(audit.entries)).toEqual(before);
    expect(audit.newCount).toBe(3);
    const signal = audit.topSignal;
    await audit.showPending();
    expect(audit.newCount).toBe(0);
    expect(audit.atTop).toBe(true);
    expect(audit.entries).toHaveLength(103);
    expect(audit.topSignal).toBe(signal + 1);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("revenu en haut à la main : ce qui attendait rejoint la liste", async () => {
    const { audit, bridge } = await opened(120);
    audit.setAtTop(false);
    bridge.audit.add("forge");
    expect(audit.newCount).toBe(1);
    audit.setAtTop(true);
    await flushPromises();
    expect(audit.newCount).toBe(0);
    expect(audit.entries).toHaveLength(101);
  });

  it("jamais d'entrée doublée entre la page chargée et le flux : une entrée arrivée PENDANT la lecture est gardée une fois", async () => {
    const ctx = await startedApp();
    ctx.bridge.audit.seed("forge", 30);
    const original = ctx.bridge.readAudit.bind(ctx.bridge);
    let injected: AuditEntry | null = null;
    ctx.bridge.readAudit = async (...args) => {
      const page = await original(...args);
      // Écrite après la lecture, livrée par le flux avant la réponse : à garder, sans doublon.
      injected ??= ctx.bridge.audit.add("forge", { account: "paul" });
      return page;
    };
    const audit = useAuditStore();
    await audit.open("forge");
    await flushPromises();
    expect(audit.entries).toHaveLength(31);
    expect(audit.entries[0]?.id).toBe((injected as AuditEntry | null)?.id);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("au-delà de la file d'attente, on le dit et on relit la tête au clic (rien de perdu)", async () => {
    const { audit, bridge } = await opened(150);
    audit.setAtTop(false);
    for (let i = 0; i < AUDIT_PENDING_MAX + 50; i += 1) bridge.audit.add("forge");
    expect(audit.newCount).toBe(AUDIT_PENDING_MAX);
    expect(audit.pendingOverflow).toBe(true);
    await audit.showPending();
    expect(audit.pendingOverflow).toBe(false);
    // La tête se relit (la plus récente du journal en haut) ; le reste se charge en descendant.
    expect(audit.entries[0]?.id).toBe(150 + AUDIT_PENDING_MAX + 50);
    expect(audit.hasMoreBelow).toBe(true);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("annonce les nouvelles entrées aux lecteurs d'écran au plus une fois par intervalle", async () => {
    const { audit, bridge } = await opened(10);
    audit.setAtTop(false);
    bridge.audit.add("forge");
    expect(audit.announcement).toBe("1 nouvelle entrée");
    for (let i = 0; i < 4; i += 1) bridge.audit.add("forge");
    expect(audit.announcement).toBe("1 nouvelle entrée");
    await vi.advanceTimersByTimeAsync(2100);
    expect(audit.announcement).toBe("5 nouvelles entrées");
  });
});

describe("filtres appliqués au direct (BR-AUDIT-014)", () => {
  it("une entrée qui ne correspond pas au filtre n'apparaît jamais, une qui correspond arrive (par l'agent)", async () => {
    const { audit, bridge } = await opened(120);
    expect(await audit.apply({ ...emptyDraft(), accounts: ["marie"] })).toBe("applied");
    const count = audit.entries.length;
    expect(audit.entries.every((entry) => entry.account === "marie")).toBe(true);
    const stranger = bridge.audit.add("forge", { account: "paul" });
    const mine = bridge.audit.add("forge", { account: "marie" });
    await vi.advanceTimersByTimeAsync(AUDIT_LIVE_DEBOUNCE_MS + 50);
    await flushPromises();
    expect(ids(audit.entries)).not.toContain(stranger.id);
    expect(audit.entries[0]?.id).toBe(mine.id);
    expect(audit.entries).toHaveLength(count + 1);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("une rafale d'entrées filtrées ne fait qu'une relecture, pas une par entrée", async () => {
    const { audit, bridge } = await opened(60);
    await audit.apply({ ...emptyDraft(), text: "marie" });
    const reads = bridge.audit.reads.length;
    for (let i = 0; i < 20; i += 1) bridge.audit.add("forge", { account: "marie" });
    await vi.advanceTimersByTimeAsync(AUDIT_LIVE_DEBOUNCE_MS + 50);
    await flushPromises();
    expect(bridge.audit.reads.length - reads).toBeLessThanOrEqual(2);
    expect(audit.entries.filter((e) => e.account === "marie").length).toBe(audit.entries.length);
  });

  it("la recherche est celle de l'agent : insensible à la casse et aux accents, début de mot", async () => {
    const { audit } = await opened(60);
    await audit.apply({ ...emptyDraft(), text: "LEA" });
    expect(audit.entries.length).toBeGreaterThan(0);
    expect(audit.entries.every((entry) => entry.account === "léa")).toBe(true);
  });

  it("une période invalide ne lance rien ; un échec de lecture garde les filtres précédents et la liste", async () => {
    const { audit, bridge } = await opened(60);
    const reads = bridge.audit.reads.length;
    expect(
      await audit.apply({
        ...emptyDraft(),
        period: "custom",
        fromDate: "2026-10-03",
        toDate: "2026-10-01",
      }),
    ).toBe("invalid");
    expect(bridge.audit.reads).toHaveLength(reads);
    await audit.apply({ ...emptyDraft(), accounts: ["paul"] });
    const kept = ids(audit.entries);
    const appliedBefore = audit.applied;
    bridge.audit.failReads = true;
    expect(await audit.apply({ ...emptyDraft(), accounts: ["marie"] })).toBe("failed");
    expect(audit.status).toBe("error");
    expect(audit.applied).toEqual(appliedBefore);
    expect(ids(audit.entries)).toEqual(kept);
    bridge.audit.failReads = false;
    expect(await audit.clearFilters()).toBe(true);
    expect(audit.status).toBe("ready");
    expect(audit.applied.accounts).toEqual([]);
  });
});

describe("coupure et retour du lien (BR-AUDIT-011, 020)", () => {
  it("garde ce qui est affiché pendant la coupure ; au retour, rattrape les entrées manquées, sans doublon ni trou", async () => {
    const { audit, bridge } = await opened(150);
    const shown = ids(audit.entries);
    bridge.setState("forge", "offline");
    const missed = [1, 2, 3].map(() => bridge.audit.add("forge"));
    expect(ids(audit.entries)).toEqual(shown);
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(ids(audit.entries.slice(0, 3))).toEqual(missed.map((entry) => entry.id).reverse());
    expect(audit.entries).toHaveLength(103);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Lien rétabli, données à jour",
    );
  });

  it("une coupure longue (plus de pages que le plafond) repart de la tête : la liste reste une suite contiguë", async () => {
    const { audit, bridge } = await opened(150);
    bridge.setState("forge", "offline");
    for (let i = 0; i < 1100; i += 1) bridge.audit.add("forge");
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(audit.status).toBe("ready");
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    // La tête est bien la plus récente du journal.
    const head = await bridge.audit.read("forge", JSON.parse(JSON.stringify(audit.applied)), null);
    expect(audit.entries[0]?.id).toBe(head.events[0]?.id);
  });

  it("une lecture hors « Connecté » échoue sans rien envoyer ; les données restent", async () => {
    const { audit, bridge } = await opened(60);
    bridge.setState("forge", "offline");
    await audit.apply({ ...emptyDraft(), accounts: ["marie"] });
    expect(audit.status).toBe("error");
    expect(audit.failure).toEqual({ kind: "not_connected" });
    expect(audit.entries.length).toBeGreaterThan(0);
  });

  it("« Rechargement manuel » : une tentative de reconnexion, puis « Serveur toujours injoignable » si le lien ne revient pas", async () => {
    const { audit, bridge } = await opened(20);
    bridge.setState("forge", "offline");
    const pending = audit.reloadManually();
    expect(audit.reloading).toBe(true);
    await vi.advanceTimersByTimeAsync(6000);
    await pending;
    expect(audit.reloading).toBe(false);
    expect(bridge.retries).toContain("forge");
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Serveur toujours injoignable",
    );
  });
});

describe("export (BR-AUDIT-017)", () => {
  it("exporte le résultat filtré APPLIQUÉ, pas un brouillon ; « Export terminé » à l'enregistrement", async () => {
    const { audit, bridge } = await opened(60);
    await audit.apply({ ...emptyDraft(), accounts: ["paul"], outcomes: ["ok"] });
    await audit.exportCsv();
    expect(bridge.audit.exports).toHaveLength(1);
    expect(bridge.audit.exports[0]?.filter).toMatchObject({ accounts: ["paul"], outcomes: ["ok"] });
    expect(useToastsStore().items.map((toast) => toast.message)).toContain("Export terminé");
    expect(audit.exporting).toBe(false);
  });

  it("ne dit rien quand l'utilisateur ferme la boîte d'enregistrement ; dit l'échec sinon ; avertit d'un fichier tronqué", async () => {
    const { audit, bridge } = await opened(20);
    const toasts = useToastsStore();
    bridge.audit.exportMode = "cancel";
    await audit.exportCsv();
    expect(toasts.items).toHaveLength(0);
    bridge.audit.exportMode = "fail";
    await audit.exportCsv();
    expect(toasts.items.map((toast) => toast.message)).toContain("Impossible de générer l'export");
    toasts.items.splice(0);
    bridge.audit.exportMode = "save";
    bridge.audit.exportTruncated = true;
    await audit.exportCsv();
    expect(toasts.items.map((toast) => toast.message)).toContain(
      "Le fichier contient seulement les 10 000 entrées les plus récentes du résultat.",
    );
  });
});

describe("cycle de vie", () => {
  it("fermer relâche l'écoute et vide tout : filtres et liste ne survivent pas à la fermeture", async () => {
    const { audit, bridge } = await opened(60);
    await audit.apply({ ...emptyDraft(), accounts: ["marie"] });
    expect(bridge.audit.listenerCount("forge")).toBe(1);
    await audit.close();
    expect(bridge.audit.listenerCount("forge")).toBe(0);
    expect(audit.entries).toHaveLength(0);
    expect(audit.applied.accounts).toEqual([]);
    expect(audit.serverId).toBeNull();
    await audit.open("forge");
    await flushPromises();
    expect(audit.applied.accounts).toEqual([]);
    expect(bridge.audit.listenerCount("forge")).toBe(1);
  });

  it("une réponse tardive d'un filtre remplacé est écartée", async () => {
    const ctx = await startedApp();
    ctx.bridge.audit.seed("forge", 40);
    const audit = useAuditStore();
    await audit.open("forge");
    await flushPromises();
    const original = ctx.bridge.readAudit.bind(ctx.bridge);
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    ctx.bridge.readAudit = async (...args) => {
      if (args[1].accounts.includes("paul")) await gate;
      return original(...args);
    };
    const slow = audit.apply({ ...emptyDraft(), accounts: ["paul"] });
    const fast = audit.apply({ ...emptyDraft(), accounts: ["marie"] });
    await fast;
    release();
    await slow;
    await flushPromises();
    expect(audit.entries.every((entry) => entry.account === "marie")).toBe(true);
  });
  it("deux applications qui se chevauchent : la première, résolue en dernier, ne restaure rien et ne notifie rien", async () => {
    const ctx = await startedApp();
    ctx.bridge.audit.seed("forge", 40);
    const audit = useAuditStore();
    await audit.open("forge");
    await flushPromises();
    const original = ctx.bridge.readAudit.bind(ctx.bridge);
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    ctx.bridge.readAudit = async (...args) => {
      if (args[1].accounts.includes("paul")) await gate;
      return original(...args);
    };
    const slow = audit.apply({ ...emptyDraft(), accounts: ["paul"] });
    const fast = audit.apply({ ...emptyDraft(), accounts: ["marie"] });
    expect(await fast).toBe("applied");
    release();
    // Dépassée : ni « failed » (qui ferait notifier une erreur), ni restauration du filtre d'avant.
    expect(await slow).toBe("superseded");
    await flushPromises();
    expect(audit.applied.accounts).toEqual(["marie"]);
    expect(audit.status).toBe("ready");
  });
});

describe("mergeDesc", () => {
  it("ne double jamais une entrée et trie de la plus récente à la plus ancienne", () => {
    const make = (id: number) => ({ id }) as AuditEntry;
    const merged = mergeDesc([make(5), make(3)], [make(4), make(5), make(6)]);
    expect(merged.map((e) => e.id)).toEqual([6, 5, 4, 3]);
  });
});

describe("continuité vérifiée (le journal est une preuve)", () => {
  async function settle(ms = AUDIT_LIVE_DEBOUNCE_MS + 50) {
    await vi.advanceTimersByTimeAsync(ms);
    await flushPromises();
  }

  it("une entrée perdue au milieu d'une rafale : la suivante la révèle, la relecture comble le trou à sa place", async () => {
    const { audit, bridge } = await opened(120);
    const a = bridge.audit.add("forge", { actionLabel: "A", action: "logout" });
    const lost = bridge.audit.add("forge", { actionLabel: "Perdue", action: "logout" }, false);
    const c = bridge.audit.add("forge", { actionLabel: "C", action: "logout" });
    // Tout de suite : la liste montre A et C, mais le doute est connu (rien ne prouve la continuité).
    expect(ids(audit.entries).slice(0, 2)).toEqual([c.id, a.id]);
    expect(audit.unverified).toBe(true);
    await settle();
    expect(ids(audit.entries).slice(0, 3)).toEqual([c.id, lost.id, a.id]);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(audit.unverified).toBe(false);
  });

  it("le trou est comblé à sa place même quand l'utilisateur a défilé", async () => {
    const { audit, bridge } = await opened(120);
    audit.setAtTop(false);
    bridge.audit.add("forge");
    bridge.audit.add("forge", {}, false);
    bridge.audit.add("forge");
    await settle();
    await audit.showPending();
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
    expect(audit.entries).toHaveLength(103);
  });

  it("un avis de retard du flux (Lagged) déclenche la relecture même sans entrée suivante", async () => {
    const { audit, bridge } = await opened(60);
    const lost = bridge.audit.add("forge", {}, false);
    expect(ids(audit.entries)).not.toContain(lost.id);
    bridge.audit.gap("forge");
    await settle();
    expect(audit.entries[0]?.id).toBe(lost.id);
  });

  it("la dernière entrée perdue sans avis ni suivante est retrouvée par la vérification périodique", async () => {
    const { audit, bridge } = await opened(60);
    const lost = bridge.audit.add("forge", {}, false);
    await settle();
    expect(ids(audit.entries)).not.toContain(lost.id);
    await vi.advanceTimersByTimeAsync(AUDIT_VERIFY_EVERY_MS + 100);
    await flushPromises();
    expect(audit.entries[0]?.id).toBe(lost.id);
  });

  it("la vérification périodique ne se fait pas hors « Connecté » (aucune relecture automatique pendant la coupure)", async () => {
    const { audit, bridge } = await opened(20);
    bridge.setState("forge", "offline");
    const reads = bridge.audit.reads.length;
    await vi.advanceTimersByTimeAsync(AUDIT_VERIFY_EVERY_MS * 3);
    expect(bridge.audit.reads).toHaveLength(reads);
    expect(audit.status).toBe("ready");
  });

  it("un rattrapage qui échoue SE VOIT : état d'erreur, liste gardée, le doute reste ; « Réessayer » comble", async () => {
    const { audit, bridge } = await opened(60);
    const lost = bridge.audit.add("forge", {}, false);
    bridge.audit.failReads = true;
    const next = bridge.audit.add("forge");
    await settle();
    expect(audit.status).toBe("error");
    expect(audit.unverified).toBe(true);
    expect(audit.entries[0]?.id).toBe(next.id);
    expect(ids(audit.entries)).not.toContain(lost.id);
    // Une autre entrée en direct ne fait pas croire que tout va bien.
    bridge.audit.add("forge");
    await settle();
    expect(audit.status).toBe("error");
    bridge.audit.failReads = false;
    await audit.retry();
    await flushPromises();
    expect(audit.status).toBe("ready");
    expect(ids(audit.entries)).toContain(lost.id);
    expect(audit.unverified).toBe(false);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("coupure puis retour SANS entrée manquée : rien ne change ; AVEC : elles sont ajoutées", async () => {
    const { audit, bridge } = await opened(60);
    const before = ids(audit.entries);
    bridge.setState("forge", "offline");
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(ids(audit.entries)).toEqual(before);
    bridge.setState("forge", "offline");
    const missed = bridge.audit.add("forge", {}, false);
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(audit.entries[0]?.id).toBe(missed.id);
    expect(audit.entries).toHaveLength(before.length + 1);
  });

  it("un trou de plus de 10 pages, utilisateur en haut : la liste repart de la tête en silence", async () => {
    const { audit, bridge } = await opened(150);
    const signal = audit.topSignal;
    bridge.setState("forge", "offline");
    for (let i = 0; i < 1100; i += 1) bridge.audit.add("forge", {}, false);
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(audit.topSignal).toBe(signal);
    expect(audit.status).toBe("ready");
    expect(audit.entries[0]?.id).toBe(150 + 1100);
    expect(strictlyDescendingWithoutDuplicates(audit.entries)).toBe(true);
  });

  it("un trou de plus de 10 pages, utilisateur plus bas : ce qu'il lit n'est PAS remplacé, le bouton le propose", async () => {
    const { audit, bridge } = await opened(150);
    audit.setAtTop(false);
    const reading = ids(audit.entries);
    bridge.setState("forge", "offline");
    for (let i = 0; i < 1100; i += 1) bridge.audit.add("forge", {}, false);
    bridge.setState("forge", "connected");
    audit.resume();
    await flushPromises();
    expect(ids(audit.entries)).toEqual(reading);
    expect(audit.pendingOverflow).toBe(true);
    await audit.showPending();
    expect(audit.entries[0]?.id).toBe(150 + 1100);
    expect(audit.pendingOverflow).toBe(false);
  });

  it("anti-rebond avec attente maximale : un flux continu sous filtre n'empêche pas l'affichage", async () => {
    const { audit, bridge } = await opened(40);
    await audit.apply({ ...emptyDraft(), accounts: ["marie"] });
    const reads = bridge.audit.reads.length;
    const first = bridge.audit.add("forge", { account: "marie" });
    // Une entrée toutes les 100 ms (jamais 300 ms de calme) pendant bien plus que l'attente maximale.
    for (let i = 0; i < 25; i += 1) {
      await vi.advanceTimersByTimeAsync(100);
      if (i === 21) expect(ids(audit.entries)).toContain(first.id);
      bridge.audit.add("forge", { account: "marie" });
    }
    await flushPromises();
    expect(ids(audit.entries)).toContain(first.id);
    expect(bridge.audit.reads.length - reads).toBeLessThanOrEqual(4);
    expect(AUDIT_LIVE_MAX_WAIT_MS).toBe(2000);
  });

  it("deux ouvertures rapprochées A, B, A : une seule écoute vivante, celle de A", async () => {
    const ctx = await startedApp();
    ctx.bridge.audit.seed("forge", 5);
    const audit = useAuditStore();
    const first = audit.open("forge");
    const second = audit.open("salon");
    const third = audit.open("forge");
    await Promise.all([first, second, third]);
    await flushPromises();
    expect(ctx.bridge.audit.listenerCount("forge")).toBe(1);
    expect(ctx.bridge.audit.listenerCount("salon")).toBe(0);
    expect(audit.serverId).toBe("forge");
    await audit.close();
    expect(ctx.bridge.audit.listenerCount("forge")).toBe(0);
  });

  it("un chargement de la suite qui échoue se voit et se réessaie", async () => {
    const { audit, bridge } = await opened(250);
    bridge.audit.failReads = true;
    await audit.loadMore();
    expect(audit.loadMoreFailed).toBe(true);
    expect(audit.entries).toHaveLength(100);
    bridge.audit.failReads = false;
    await audit.loadMore();
    expect(audit.loadMoreFailed).toBe(false);
    expect(audit.entries).toHaveLength(200);
  });

  it("« Aujourd'hui » appliqué puis minuit passé : la période est recalculée à la relecture", async () => {
    vi.setSystemTime(new Date(2026, 9, 4, 23, 50));
    const { audit, bridge } = await opened(40);
    await audit.apply({ ...emptyDraft(), period: "today" }, Date.now());
    expect(audit.applied.fromS).toBe(Math.floor(new Date(2026, 9, 4).getTime() / 1000));
    vi.setSystemTime(new Date(2026, 9, 5, 0, 10));
    bridge.audit.add("forge");
    await settle();
    expect(audit.applied.fromS).toBe(Math.floor(new Date(2026, 9, 5).getTime() / 1000));
  });
});
