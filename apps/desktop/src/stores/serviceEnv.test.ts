import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { logUiError } from "@/errors/report";
import { type LinkBridge, LinkCommandError, setLinkBridge } from "@/link";
import { useServiceEnvStore } from "./serviceEnv";

vi.mock("@/errors/report", () => ({ logUiError: vi.fn() }));
const noteRefusal = vi.fn();
vi.mock("./serviceRights", () => ({ useServiceRightsStore: () => ({ noteRefusal }) }));

// Le magasin des VALEURS de variables : en mémoire, d'un seul service à la fois, effacé au remasquage, jamais
// persisté ni écrit dans un rapport (HRT-63, BR-SVC-009).
const SECRET = "valeur-sentinelle-du-magasin";

function bridgeWith(reveal: (serverId: string, key: string) => Promise<unknown>) {
  const bridge = { revealServiceEnv: vi.fn(reveal) } as unknown as LinkBridge;
  setLinkBridge(bridge);
  return bridge as unknown as { revealServiceEnv: ReturnType<typeof vi.fn> };
}

beforeEach(() => setActivePinia(createPinia()));
afterEach(() => setLinkBridge(null));

describe("serviceEnv", () => {
  it("révéler : lecture, puis valeurs en mémoire pour CE service seulement", async () => {
    const bridge = bridgeWith(async () => [{ name: "A", value: SECRET }]);
    const store = useServiceEnvStore();
    const pending = store.reveal("forge", "c_1");
    expect(store.phase).toBe("reading");
    await pending;
    expect(store.phase).toBe("shown");
    expect(store.variables).toEqual([{ name: "A", value: SECRET }]);
    expect(store.target).toEqual({ serverId: "forge", key: "c_1" });
    expect(bridge.revealServiceEnv).toHaveBeenCalledWith("forge", "c_1");
  });

  it("clear EFFACE : le tableau est remplacé, pas seulement caché", async () => {
    bridgeWith(async () => [{ name: "A", value: SECRET }]);
    const store = useServiceEnvStore();
    await store.reveal("forge", "c_1");
    store.clear();
    expect(store.variables).toBeNull();
    expect(store.target).toBeNull();
    expect(store.phase).toBe("idle");
    expect(JSON.stringify(store.$state)).not.toContain(SECRET);
  });

  it("une réponse qui arrive après un remasquage est jetée", async () => {
    let release: (value: unknown) => void = () => {};
    bridgeWith(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    const store = useServiceEnvStore();
    const pending = store.reveal("forge", "c_1");
    store.clear();
    release([{ name: "A", value: SECRET }]);
    await pending;
    expect(store.variables).toBeNull();
    expect(store.phase).toBe("idle");
  });

  it("pas de seconde lecture pendant une lecture", async () => {
    const bridge = bridgeWith(async () => []);
    const store = useServiceEnvStore();
    const first = store.reveal("forge", "c_1");
    void store.reveal("forge", "c_2");
    await first;
    expect(bridge.revealServiceEnv).toHaveBeenCalledTimes(1);
  });

  it("droit retiré : « lost », valeurs masquées ; autre échec : « unreadable »", async () => {
    bridgeWith(async () => {
      throw new LinkCommandError({ kind: "missing_services_right" });
    });
    const store = useServiceEnvStore();
    await store.reveal("forge", "c_1");
    expect(store.phase).toBe("failed");
    expect(store.failure).toBe("lost");
    expect(store.variables).toBeNull();
    // Le droit affiché était périmé : il est relu tout de suite (BR-GRANT-011).
    expect(noteRefusal).toHaveBeenCalledWith("forge");
    setLinkBridge({
      revealServiceEnv: async () => {
        throw new LinkCommandError({ kind: "unreachable" });
      },
    } as unknown as LinkBridge);
    await store.reveal("forge", "c_1");
    expect(store.failure).toBe("unreadable");
  });

  it("jamais de valeur dans le journal des erreurs de l'interface", async () => {
    bridgeWith(async () => {
      throw new Error(`la réponse contenait ${SECRET}`);
    });
    const store = useServiceEnvStore();
    await store.reveal("forge", "c_1");
    expect(store.phase).toBe("failed");
    expect(logUiError).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(vi.mocked(logUiError).mock.calls)).not.toContain(SECRET);
  });
});
