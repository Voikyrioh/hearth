import { flushPromises } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useServerAction } from "@/composables/useServerAction";
import { getLinkBridge, type SimulatedLinkBridge, setLinkBridge } from "@/link";
import { startedApp } from "@/test/app";
import { MAX_OPERATIONS, RECONNECT_FAILURE_STEP } from "./link";
import { TOAST_LIFETIME_MS, useToastsStore } from "./toasts";

// HRT-12 : l'app reste utilisable hors ligne. Les règles BR-RESIL-008 à 011, 017, 018 et 020 côté
// stores ; l'aspect (bandeau, données périmées, panneaux) est dans les tests des composants.

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

describe("coupures répétées (BR-RESIL-018)", () => {
  it("compte les échecs dans UNE notification par serveur, mise à jour à chaque palier de 5, jamais à chaque tentative", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    for (let n = 1; n < RECONNECT_FAILURE_STEP; n += 1) {
      bridge.publish("forge", n > 3 ? "offline" : "reconnecting", { failedAttempts: n });
    }
    expect(toasts.items, "rien avant le premier palier").toHaveLength(0);
    bridge.publish("forge", "offline", { failedAttempts: 5 });
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.message).toBe("forge : Reconnexion échouée 5 fois.");
    expect(toasts.items[0]?.kind).toBe("warn");
    // Les tentatives du même palier ne touchent plus la notification (ni texte, ni durée).
    await vi.advanceTimersByTimeAsync(TOAST_LIFETIME_MS - 100);
    for (let n = 6; n <= 9; n += 1) bridge.publish("forge", "offline", { failedAttempts: n });
    await vi.advanceTimersByTimeAsync(200);
    expect(toasts.items, "elle disparaît seule : rien ne l'a renouvelée").toHaveLength(0);
    // Palier suivant : elle revient avec le nouveau compte, sans doublon.
    bridge.publish("forge", "offline", { failedAttempts: 10 });
    bridge.publish("forge", "offline", { failedAttempts: 11 });
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.message).toBe("forge : Reconnexion échouée 10 fois.");
  });

  it("garde un compteur par serveur et retire celui d'un serveur dont le lien est revenu", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    bridge.publish("forge", "offline", { failedAttempts: 5 });
    bridge.publish("salon", "offline", { failedAttempts: 10 });
    expect(toasts.items.map((item) => item.message)).toEqual([
      "forge : Reconnexion échouée 5 fois.",
      "nas-salon : Reconnexion échouée 10 fois.",
    ]);
    bridge.setState("forge", "connected");
    expect(toasts.items.map((item) => item.message)).toEqual([
      "nas-salon : Reconnexion échouée 10 fois.",
    ]);
    // Une nouvelle coupure repart de zéro.
    bridge.publish("forge", "offline", { failedAttempts: 5 });
    expect(toasts.items).toHaveLength(2);
  });

  it("repart de zéro après une session expirée et oublie un serveur supprimé", async () => {
    const { bridge, servers } = await startedApp();
    const toasts = useToastsStore();
    bridge.publish("forge", "offline", { failedAttempts: 10 });
    expect(toasts.items).toHaveLength(1);
    // Session expirée : la panne est finie, la notification part, le palier aussi.
    bridge.publish("forge", "session_expired", { reason: "expired", failedAttempts: 0 });
    expect(toasts.items).toHaveLength(0);
    bridge.publish("forge", "offline", { failedAttempts: 5 });
    expect(toasts.items[0]?.message).toBe("forge : Reconnexion échouée 5 fois.");
    // Serveur supprimé : la notification et son palier sont retirés.
    bridge.dropServer("forge");
    await flushPromises();
    expect(servers.byId("forge")).toBeUndefined();
    expect(toasts.items).toHaveLength(0);
  });

  it("ne dit rien d'une coupure courte, ni d'une session expirée ou d'un accès révoqué", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    bridge.publish("forge", "reconnecting", { failedAttempts: 4 });
    bridge.publish("forge", "session_expired", { reason: "expired", failedAttempts: 0 });
    bridge.publish("salon", "access_revoked", { reason: "revoked" });
    expect(toasts.items).toHaveLength(0);
  });
});

describe("notification à clé", () => {
  it("se met à jour sur place sans ajouter de ligne, se renouvelle et se retire par sa clé", async () => {
    await startedApp();
    const toasts = useToastsStore();
    const first = toasts.push({ key: "k", kind: "warn", message: "un" });
    const again = toasts.push({ key: "k", kind: "warn", message: "deux" });
    expect(again).toBe(first);
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.message).toBe("deux");
    // Le délai est renouvelé par la mise à jour.
    await vi.advanceTimersByTimeAsync(TOAST_LIFETIME_MS - 100);
    toasts.push({ key: "k", kind: "warn", message: "trois" });
    await vi.advanceTimersByTimeAsync(TOAST_LIFETIME_MS - 100);
    expect(toasts.items).toHaveLength(1);
    toasts.dismissKey("k");
    expect(toasts.items).toHaveLength(0);
    toasts.dismissKey("absente");
  });
});

describe("action lancée au moment d'une coupure (BR-RESIL-009, 010)", () => {
  it("dit que le résultat n'est pas connu, ne rejoue jamais, puis annonce l'issue au retour du lien", async () => {
    const { bridge, link } = await startedApp();
    const toasts = useToastsStore();
    const action = useServerAction();
    bridge.actionMode = "cut";
    const result = await action.run(() => bridge.runDevAction("forge"));
    expect(result).toMatchObject({ kind: "unknown" });
    expect(toasts.items.map((item) => item.message)).toEqual([
      "Le résultat de cette action n'est pas connu.",
      "Vérifie l'état du serveur, puis relance l'action si besoin.",
    ]);
    expect(link.stateOf("forge")).toBe("reconnecting");
    const opId = result?.kind === "unknown" ? result.opId : null;
    expect(opId).toBe(bridge.lastUnknownOpId);
    // Le lien revient : la bibliothèque a lu `/operations/{id}` et annonce l'issue.
    bridge.setState("forge", "connected");
    bridge.emitOperation({ opId: opId ?? "", serverId: "forge", outcome: "done" });
    expect(link.outcomeOf(opId ?? "")).toBe("done");
    expect(toasts.items.at(-1)?.message).toBe("forge : Fait pendant la coupure.");
    // Jamais rejouée : une seule action est partie.
    expect(bridge.calls.filter((call) => call.startsWith("action "))).toEqual(["action dev-ping"]);
  });

  it("annonce les trois issues avec les textes de la spec", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    bridge.emitOperation({ opId: "a", serverId: "forge", outcome: "done" });
    bridge.emitOperation({ opId: "b", serverId: "forge", outcome: "not_executed" });
    bridge.emitOperation({ opId: "c", serverId: "forge", outcome: "unknown" });
    expect(toasts.items.map((item) => item.message)).toEqual([
      "forge : Fait pendant la coupure.",
      "forge : Non exécuté. Tu peux relancer.",
      "forge : Résultat inconnu. Vérifie l'état du serveur.",
    ]);
  });

  it("refuse sans rien envoyer hors « Connecté », par une notification discrète (BR-RESIL-008, 011)", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    const action = useServerAction();
    bridge.setState("forge", "offline");
    const result = await action.run(() => bridge.runDevAction("forge"));
    expect(result).toBeNull();
    expect(toasts.items.map((item) => item.kind)).toEqual(["error"]);
    expect(toasts.items[0]?.message).toBe(
      "Indisponible tant que le lien avec le serveur n'est pas établi.",
    );
  });

  it("répond normalement quand le lien tient", async () => {
    await startedApp();
    const action = useServerAction();
    const bridge = getLinkBridge() as SimulatedLinkBridge;
    expect(await action.run(() => bridge.runDevAction("forge"))).toEqual({
      kind: "completed",
      status: 200,
      body: "{}",
    });
    expect(action.busy.value).toBe(false);
  });
});

describe("un état de lien par serveur (BR-RESIL-020)", () => {
  it("la coupure de l'un ne touche pas l'autre", async () => {
    const { bridge, link } = await startedApp();
    bridge.setState("forge", "offline");
    expect(link.stateOf("forge")).toBe("offline");
    expect(link.stateOf("salon")).toBe("connected");
    bridge.setState("salon", "reconnecting");
    bridge.setState("forge", "connected");
    expect(link.stateOf("forge")).toBe("connected");
    expect(link.stateOf("salon")).toBe("reconnecting");
  });
});

describe("session de plusieurs jours (BR-RESIL-017)", () => {
  it("ne fait grossir ni les notifications, ni les états, ni les issues", async () => {
    const { bridge, link } = await startedApp();
    const toasts = useToastsStore();
    // Trois jours de coupures toutes les 10 minutes sur deux serveurs, avec une opération chacune.
    for (let n = 0; n < 3 * 24 * 6; n += 1) {
      for (const id of ["forge", "salon"]) {
        bridge.publish(id, "reconnecting", { failedAttempts: 3 });
        bridge.publish(id, "offline", { failedAttempts: 9 });
        bridge.emitOperation({ opId: `${id}-${n}`, serverId: id, outcome: "unknown" });
        bridge.setState(id, "connected");
      }
      await vi.advanceTimersByTimeAsync(10 * 60_000);
    }
    await flushPromises();
    expect(Object.keys(link.events)).toHaveLength(2);
    expect(Object.keys(link.operations).length).toBeLessThanOrEqual(MAX_OPERATIONS);
    expect(toasts.items.length).toBeLessThanOrEqual(50);
  });
});
