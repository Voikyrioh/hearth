import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { watch } from "vue";
import { getLinkBridge, type ServiceRights, SimulatedLinkBridge } from "@/link";
import { freshBridge, startedApp } from "@/test/app";
import { RIGHTS_READ_LIMIT_MS, useServiceRightsStore } from "./serviceRights";

// Le droit « gérer les services » du compte connecté, par serveur (HRT-78, BR-GRANT-011) : lu à l'ouverture, relu
// à chaque reconnexion et après un refus, jamais écrit ; lien coupé il garde les derniers droits connus.
beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] }));
afterEach(() => vi.useRealTimers());

async function connected() {
  const { bridge } = freshBridge({ retryDelayMs: -1 });
  const store = useServiceRightsStore();
  return { bridge, store };
}

describe("lecture du droit", () => {
  it("la première lecture passe par « en cours », puis dit le droit du compte connecté", async () => {
    const { store } = await connected();
    const pending = store.load("forge");
    expect(store.of("forge")).toEqual({ phase: "reading", rights: null });
    await pending;
    expect(store.of("forge")).toEqual({
      phase: "known",
      rights: { known: true, granted: true, gameSettings: true, account: "marie" },
    });
  });

  it("une relecture garde le dernier droit connu pendant qu'elle s'exécute, puis le remplace", async () => {
    const { bridge, store } = await connected();
    await store.load("forge");
    bridge.setServiceRightOnServer("forge", "marie", false);
    const again = store.load("forge");
    expect(store.of("forge")?.phase).toBe("known");
    expect(store.of("forge")?.rights?.granted).toBe(true);
    await again;
    expect(store.of("forge")?.rights?.granted).toBe(false);
  });

  it("après un refus pour absence du droit, le droit est relu tout de suite (BR-GRANT-011)", async () => {
    const { bridge, store } = await connected();
    await store.load("forge");
    const before = bridge.rightsReads.length;
    bridge.setServiceRightOnServer("forge", "marie", false);
    await store.noteRefusal("forge");
    expect(bridge.rightsReads.length).toBe(before + 1);
    expect(store.of("forge")?.rights?.granted).toBe(false);
  });

  it("une lecture plus ancienne qu'une autre n'écrase jamais la plus récente", async () => {
    const { store } = await connected();
    let release: (rights: ServiceRights) => void = () => {};
    const bridge = getLinkBridge();
    const spy = vi
      .spyOn(bridge, "readServiceRights")
      .mockImplementationOnce(
        () =>
          new Promise<ServiceRights>((resolve) => {
            release = resolve;
          }),
      )
      .mockImplementationOnce(async () => ({
        known: true,
        granted: false,
        gameSettings: true,
        account: "marie",
      }));
    const slow = store.load("forge");
    await store.load("forge");
    release({ known: true, granted: true, gameSettings: true, account: "marie" });
    await slow;
    expect(store.of("forge")?.rights?.granted).toBe(false);
    expect(spy).toHaveBeenCalledTimes(2);
  });
});

describe("échec et lien coupé", () => {
  it("une lecture en échec n'invente aucun droit : l'état est « échec », sans droit", async () => {
    const { bridge, store } = await connected();
    bridge.failRightsReads = true;
    await store.load("forge");
    expect(store.of("forge")).toEqual({ phase: "failed", rights: null });
  });

  it("une lecture qui dépasse 5 secondes est dite en échec (état UI)", async () => {
    const { store } = await connected();
    vi.spyOn(getLinkBridge(), "readServiceRights").mockImplementation(
      () => new Promise<ServiceRights>(() => {}),
    );
    const pending = store.load("forge");
    expect(store.of("forge")?.phase).toBe("reading");
    await vi.advanceTimersByTimeAsync(RIGHTS_READ_LIMIT_MS + 1);
    await pending;
    expect(store.of("forge")?.phase).toBe("failed");
  });

  it("lien coupé : les derniers droits connus restent, ce n'est pas un échec de lecture", async () => {
    const { bridge, store } = await connected();
    await store.load("forge");
    bridge.setState("forge", "offline");
    await store.load("forge");
    expect(store.of("forge")).toEqual({
      phase: "known",
      rights: { known: true, granted: true, gameSettings: true, account: "marie" },
    });
  });

  it("lien coupé sans rien de connu : l'échec est dit (les boutons restent désactivés)", async () => {
    const { bridge, store } = await connected();
    bridge.setState("forge", "offline");
    await store.load("forge");
    await flushPromises();
    expect(store.of("forge")?.phase).toBe("failed");
  });

  it("un serveur oublié n'a plus de droit en mémoire", async () => {
    const { store } = await connected();
    await store.load("forge");
    store.forget("forge");
    expect(store.of("forge")).toBeUndefined();
  });
});

describe("le magasin ne fait que lire", () => {
  it("n'expose aucune action qui accorde ou retire le droit (BR-GRANT-005)", async () => {
    const { store } = await connected();
    const names = Object.keys(store).filter((key) => typeof (store as never)[key] === "function");
    expect(names.filter((name) => /grant|revoke|give|set.*right/i.test(name))).toEqual([]);
    expect(SimulatedLinkBridge.prototype.readServiceRights).toBeTypeOf("function");
  });
});

// C3 de la review : le droit est celui d'UN compte. Après un changement de compte ou de session, le compte B ne voit
// jamais le droit ni l'identifiant du compte A, même un instant (l'ancienne entrée n'est rendue pour personne).
describe("changement de compte ou de session", () => {
  const PAUL = { password: "Correct-Horse-9", role: "readonly" as const };
  async function twoAccounts() {
    const ctx = await startedApp({
      retryDelayMs: -1,
      agents: [
        {
          host: "192.168.1.120",
          fingerprint: "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90",
          users: { marie: PAUL, paul: PAUL },
        },
      ],
    });
    const store = useServiceRightsStore();
    await store.load("forge");
    expect(store.of("forge")?.rights).toEqual({
      known: true,
      granted: true,
      gameSettings: true,
      account: "marie",
    });
    return { ...ctx, store };
  }

  it("après la déconnexion de A puis la connexion de B, B ne voit jamais le droit de A, même un instant", async () => {
    const { bridge, store } = await twoAccounts();
    const seen: Array<ReturnType<typeof store.of>> = [];
    watch(
      () => store.of("forge"),
      (value) => seen.push(value),
      { flush: "sync" },
    );
    await bridge.logout("forge");
    await flushPromises();
    // La session de A est fermée : rien n'est gardé.
    expect(store.of("forge")).toBeUndefined();
    await bridge.login("forge", "paul", "Correct-Horse-9", false);
    await flushPromises();
    // Connecté en B, avant toute lecture : rien de A.
    expect(store.of("forge")).toBeUndefined();
    await store.load("forge");
    expect(store.of("forge")?.rights).toEqual({
      known: true,
      granted: false,
      gameSettings: true,
      account: "paul",
    });
    const shown = JSON.stringify(seen);
    expect(shown).not.toContain("marie");
    expect(shown).not.toContain('"granted":true');
  });

  it("changement de compte sans quitter l'état connecté : l'entrée de A n'est plus rendue", async () => {
    const { bridge, store } = await twoAccounts();
    await bridge.login("forge", "paul", "Correct-Horse-9", false);
    await flushPromises();
    expect(store.of("forge")).toBeUndefined();
  });

  it("une lecture de A qui arrive après la connexion de B est écartée", async () => {
    const { bridge, store } = await twoAccounts();
    let release: (rights: ServiceRights) => void = () => {};
    vi.spyOn(bridge, "readServiceRights").mockImplementationOnce(
      () =>
        new Promise<ServiceRights>((resolve) => {
          release = resolve;
        }),
    );
    const late = store.load("forge");
    await bridge.login("forge", "paul", "Correct-Horse-9", false);
    await flushPromises();
    release({ known: true, granted: true, gameSettings: true, account: "marie" });
    await late;
    expect(JSON.stringify(store.of("forge") ?? null)).not.toContain("marie");
  });

  it("session expirée ou accès révoqué : le droit est oublié", async () => {
    const { bridge, store } = await twoAccounts();
    bridge.setState("forge", "access_revoked");
    await flushPromises();
    expect(store.of("forge")).toBeUndefined();
  });

  it("l'autre serveur garde le sien", async () => {
    const { bridge, store } = await twoAccounts();
    await store.load("salon");
    await bridge.logout("forge");
    await flushPromises();
    expect(store.of("forge")).toBeUndefined();
    expect(store.of("salon")?.rights?.account).toBe("paul");
  });
});
