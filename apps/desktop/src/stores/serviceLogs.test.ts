import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SAMPLE_AGENT, SimulatedLinkBridge, setLinkBridge } from "@/link";
import { MAX_LINES } from "@/logs/buffer";
import { useServiceLogsStore } from "./serviceLogs";

// La lecture des journaux d'un service (HRT-67) sur le pont simulé : direct, pause qui fige et compte exactement,
// reprise, plafond de 5 000 lignes, coupure et reprise sans doublon, refus (plafond du compte, frein), effacement à la
// sortie. Rien d'une ligne ne doit rester en mémoire une fois la lecture fermée.
const KEY = "c_web";
let bridge: SimulatedLinkBridge;

beforeEach(() => {
  setActivePinia(createPinia());
  bridge = new SimulatedLinkBridge({ agents: [SAMPLE_AGENT] });
  setLinkBridge(bridge);
});
afterEach(() => {
  bridge.logs.reset();
  setLinkBridge(null);
});

const texts = (store: ReturnType<typeof useServiceLogsStore>) =>
  store
    .entries()
    .filter((e) => e.kind === "line")
    .map((e) => (e.kind === "line" ? e.text : ""));

async function opened() {
  const store = useServiceLogsStore();
  await store.open("forge", KEY);
  await vi.waitFor(() => expect(store.phase).toBe("reading"));
  return store;
}

describe("direct, pause et reprise (BR-SLOG-003 à 005)", () => {
  it("affiche la première page puis les lignes qui arrivent, nettoyées", async () => {
    bridge.logs.seed("forge", KEY, [{ text: "un" }, { text: "deux" }]);
    const store = await opened();
    await vi.waitFor(() => expect(texts(store)).toEqual(["un", "deux"]));
    bridge.logs.push("forge", KEY, [{ text: `\u001b[31mtrois\u001b[0m` }]);
    await vi.waitFor(() => expect(texts(store)).toEqual(["un", "deux", "trois"]));
    expect(store.received).toBe(true);
  });

  it("la pause fige l'écran et compte exactement ; la reprise insère dans l'ordre", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "avant" }]);
    await vi.waitFor(() => expect(texts(store)).toEqual(["avant"]));
    store.pause("manual");
    bridge.logs.push("forge", KEY, [{ text: "a" }, { text: "b" }]);
    bridge.logs.push("forge", KEY, [{ text: "c" }]);
    await vi.waitFor(() => expect(store.waiting).toBe(3));
    expect(texts(store)).toEqual(["avant"]);
    store.resume();
    expect(texts(store)).toEqual(["avant", "a", "b", "c"]);
    expect(store.waiting).toBe(0);
    expect(store.mode).toBe("live");
  });

  it("pendant une pause, 5 000 lignes au plus sont gardées en attente, le compteur reste exact et un trou est dit", async () => {
    const store = await opened();
    store.pause("auto");
    const lines = Array.from({ length: MAX_LINES + 1_200 }, (_, i) => ({ text: `l${i}` }));
    bridge.logs.push("forge", KEY, lines);
    await vi.waitFor(() => expect(store.waiting).toBe(MAX_LINES + 1_200));
    store.resume();
    const entries = store.entries();
    expect(entries[0]?.kind).toBe("gap");
    expect(entries[0]).toMatchObject({ gap: "pause", count: 1_200 });
    expect(texts(store)).toHaveLength(MAX_LINES);
    expect(texts(store)[0]).toBe("l1200");
  });

  it("hors pause, les plus anciennes lignes sont retirées et le retrait est dit", async () => {
    const store = await opened();
    bridge.logs.push(
      "forge",
      KEY,
      Array.from({ length: MAX_LINES + 10 }, (_, i) => ({ text: `l${i}` })),
    );
    await vi.waitFor(() => expect(store.trimmed).toBe(true));
    expect(texts(store)).toHaveLength(MAX_LINES);
  });

  it("un débit élevé arrive par lots : peu de signaux d'écran pour des milliers de lignes", async () => {
    const store = await opened();
    const before = store.version;
    for (let i = 0; i < 50; i += 1) {
      bridge.logs.push(
        "forge",
        KEY,
        Array.from({ length: 100 }, (_, n) => ({ text: `r${i}-${n}` })),
      );
    }
    await vi.waitFor(() => expect(texts(store)).toHaveLength(MAX_LINES));
    expect(store.version - before).toBeLessThan(10);
  });
});

describe("coupure, reprise, trous, redémarrage", () => {
  it("après une coupure la lecture reprend sans doublon ni perte", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "1" }, { text: "2" }]);
    await vi.waitFor(() => expect(texts(store)).toEqual(["1", "2"]));
    bridge.logs.interrupt("forge", KEY, { afterMs: 10, whileCut: [{ text: "3" }, { text: "4" }] });
    await vi.waitFor(() => expect(store.interrupted).toBe(true));
    await vi.waitFor(() => expect(texts(store)).toEqual(["1", "2", "3", "4"]));
    expect(store.interrupted).toBe(false);
  });

  it("le lien coupé interrompt la lecture et son retour la reprend", async () => {
    const store = await opened();
    bridge.setState("forge", "reconnecting");
    await vi.waitFor(() => expect(store.interrupted).toBe(true));
    bridge.logs.push("forge", KEY, [{ text: "pendant" }]);
    bridge.setState("forge", "connected");
    await vi.waitFor(() => expect(texts(store)).toEqual(["pendant"]));
  });

  it("une reprise perdue ne montre aucune ligne deux fois", async () => {
    bridge.logs.seed("forge", KEY, [
      { text: "a", t: 1 },
      { text: "b", t: 2 },
    ]);
    const store = await opened();
    await vi.waitFor(() => expect(texts(store)).toEqual(["a", "b"]));
    bridge.logs.interrupt("forge", KEY, {
      afterMs: 5,
      whileCut: [{ text: "c", t: 3 }],
      resumeLost: true,
    });
    await vi.waitFor(() => expect(texts(store)).toContain("c"));
    expect(texts(store)).toEqual(["a", "b", "c"]);
    expect(store.entries().some((e) => e.kind === "gap")).toBe(false);
  });

  it("le redémarrage met un repère, le trou lent aussi", async () => {
    const store = await opened();
    bridge.logs.restart("forge", KEY);
    bridge.logs.gap("forge", KEY, "slow", 40);
    await vi.waitFor(() => expect(store.entries().map((e) => e.kind)).toEqual(["restart", "gap"]));
  });

  it("un état de lecture inconnu est gardé tel quel, sans rien en déduire", async () => {
    const store = await opened();
    bridge.logs.setStatus("forge", KEY, { state: "unknown" });
    await vi.waitFor(() => expect(store.status?.state).toBe("unknown"));
  });
});

describe("refus : jamais retentés en boucle", () => {
  it("le quatrième onglet est refusé avec son code, la place libérée est rendue", async () => {
    const readings = [] as Array<ReturnType<typeof useServiceLogsStore>>;
    for (const key of ["a", "b", "c"]) {
      const reading = await bridge.openServiceLogs("forge", key, () => undefined);
      readings.push(reading as never);
    }
    await vi.waitFor(() => expect(bridge.logs.openCount()).toBe(3));
    const store = useServiceLogsStore();
    await store.open("forge", "d");
    await vi.waitFor(() => expect(store.phase).toBe("ended"));
    expect(store.end).toEqual({
      kind: "refused",
      refusal: "logs_account_limit",
      retryAfterS: null,
    });
    expect(bridge.logs.opened.filter((o) => o.key === "d")).toHaveLength(1);
  });

  it("le frein donne son délai et n'est jamais rouvert seul", async () => {
    bridge.logs.refuseNext("logs_rate_limited", 4);
    const store = useServiceLogsStore();
    await store.open("forge", KEY);
    await vi.waitFor(() => expect(store.phase).toBe("ended"));
    expect(store.end).toEqual({ kind: "refused", refusal: "logs_rate_limited", retryAfterS: 4 });
    expect(store.retryAt).toBeGreaterThan(Date.now() + 3_000);
    await new Promise((r) => setTimeout(r, 150));
    expect(bridge.logs.opened).toHaveLength(1);
  });

  it("le droit retiré efface les lignes déjà reçues", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "secret=abc" }]);
    await vi.waitFor(() => expect(texts(store)).toHaveLength(1));
    bridge.logs.end("forge", KEY, { kind: "refused", refusal: "missing_right", retryAfterS: null });
    await vi.waitFor(() => expect(store.phase).toBe("ended"));
    expect(store.entries()).toEqual([]);
  });

  it("une ouverture en échec dit pourquoi sans citer le serveur", async () => {
    bridge.logs.failOpen = true;
    const store = useServiceLogsStore();
    await store.open("forge", KEY);
    expect(store.phase).toBe("ended");
    expect(store.failure).toBe("unreachable");
  });
});

describe("quitter efface tout (BR-SLOG-019, BR-SLOG-020)", () => {
  it("fermer remplace le tampon, ferme la lecture et rend la place", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "mot de passe: hunter2" }]);
    await vi.waitFor(() => expect(texts(store)).toHaveLength(1));
    expect(bridge.logs.openCount()).toBe(1);
    store.close();
    expect(store.entries()).toEqual([]);
    expect(store.phase).toBe("idle");
    expect(bridge.logs.openCount()).toBe(0);
    expect(bridge.logs.closedCount).toBe(1);
    // Une ligne tardive n'arrive plus nulle part.
    bridge.logs.push("forge", KEY, [{ text: "tard" }]);
    await new Promise((r) => setTimeout(r, 150));
    expect(store.entries()).toEqual([]);
  });

  it("une seule lecture à la fois : ouvrir un autre service ferme le précédent", async () => {
    const store = await opened();
    await store.open("forge", "c_db");
    await vi.waitFor(() => expect(store.phase).toBe("reading"));
    expect(bridge.logs.openFor("forge", KEY)).toBe(0);
    expect(bridge.logs.openFor("forge", "c_db")).toBe(1);
  });

  it("quitté pendant l'ouverture : la lecture qui arrive est fermée aussitôt", async () => {
    bridge.logs.openDelayMs = 30;
    const store = useServiceLogsStore();
    const pending = store.open("forge", KEY);
    store.close();
    await pending;
    await new Promise((r) => setTimeout(r, 100));
    expect(bridge.logs.openCount()).toBe(0);
    expect(store.phase).toBe("idle");
  });

  it("le service disparu arrête la lecture et garde les lignes", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "reste" }]);
    await vi.waitFor(() => expect(texts(store)).toEqual(["reste"]));
    store.noteGone();
    expect(store.gone).toBe(true);
    expect(bridge.logs.openCount()).toBe(0);
    expect(texts(store)).toEqual(["reste"]);
  });
});

describe("accusés", () => {
  it("chaque lot traité est acquitté", async () => {
    const store = await opened();
    bridge.logs.push("forge", KEY, [{ text: "x" }]);
    bridge.logs.push("forge", KEY, [{ text: "y" }]);
    await vi.waitFor(() => expect(texts(store)).toHaveLength(2));
    expect(bridge.logs.ackedBatches()).toBe(2);
  });
});
