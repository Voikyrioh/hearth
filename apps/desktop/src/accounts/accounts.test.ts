import { flushPromises } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { useAccountActions } from "@/composables/useAccountActions";
import { useServerAction } from "@/composables/useServerAction";
import { LinkCommandError } from "@/link";
import { useAccountsStore } from "@/stores/accounts";
import { useToastsStore } from "@/stores/toasts";
import { startedApp } from "@/test/app";
import { refusalMessage } from "./messages";

const GOOD = "Sunny-Walk-Home-42";

describe("accounts store", () => {
  it("reads the list of a server and says when the agent refuses it", async () => {
    const { bridge } = await startedApp();
    const store = useAccountsStore();
    await store.load("forge");
    expect(store.of("forge")?.status).toBe("ready");
    expect(store.of("forge")?.accounts.map((a) => a.username)).toEqual(["marie", "paul", "lea"]);
    // Un compte Lecture seule qui force la lecture : refus de l'agent, rien d'affiché.
    await store.load("salon");
    expect(store.of("salon")).toMatchObject({
      status: "refused",
      accounts: [],
      refusal: { kind: "forbidden" },
    });
    expect(bridge.calls.filter((call) => call === "account list")).toHaveLength(2);
  });

  it("keeps the last known list when a re-read fails, and never lets an older read win", async () => {
    const { bridge } = await startedApp();
    const store = useAccountsStore();
    await store.load("forge");
    vi.spyOn(bridge, "listAccounts").mockRejectedValueOnce(
      new LinkCommandError({ kind: "not_connected" }),
    );
    await store.load("forge");
    expect(store.of("forge")?.status).toBe("error");
    expect(store.of("forge")?.accounts).toHaveLength(3);

    // Deux lectures en vol : la première rend en dernier et ne doit rien écraser.
    let releaseFirst: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    vi.spyOn(bridge, "listAccounts").mockImplementationOnce(async () => {
      await gate;
      return { kind: "listed" as const, accounts: [] };
    });
    const first = store.load("forge");
    const second = store.load("forge");
    await second;
    releaseFirst();
    await first;
    expect(store.of("forge")?.accounts).toHaveLength(3);
  });
});

describe("useServerAction with an account outcome", () => {
  it("says the result is unknown once, with the text of the specification, and never replays", async () => {
    await startedApp();
    const toasts = useToastsStore();
    const { run } = useServerAction();
    const perform = vi.fn(async () => ({ kind: "unknown" as const, opId: "OP" }));
    const result = await run(perform, { unknownMessage: "accounts.unknownResult" });
    expect(result).toEqual({ kind: "unknown", opId: "OP" });
    expect(perform).toHaveBeenCalledTimes(1);
    expect(toasts.items.map((t) => t.message)).toEqual([
      "Le résultat de cette opération n'est pas connu. Elle n'a pas été rejouée automatiquement. À la reconnexion, la liste se mettra à jour.",
    ]);
  });
});

describe("account actions", () => {
  it("creates an account: success announced, list re-read, no password kept anywhere", async () => {
    const { bridge, pinia } = await startedApp();
    const store = useAccountsStore();
    const toasts = useToastsStore();
    const actions = useAccountActions(() => "forge");
    const report = await actions.create("sophie", GOOD, "readonly");
    await flushPromises();
    expect(report).toEqual({ kind: "done", sessionsClosed: 0 });
    expect(toasts.items.map((t) => t.message)).toContain("Compte sophie créé");
    expect(store.of("forge")?.accounts.map((a) => a.username)).toContain("sophie");
    // Aucun mot de passe dans le journal du pont, dans les notifications, ni dans l'état Pinia.
    expect(JSON.stringify(bridge.calls)).not.toContain(GOOD);
    expect(JSON.stringify(toasts.items)).not.toContain(GOOD);
    expect(JSON.stringify(pinia.state.value)).not.toContain(GOOD);
  });

  it("returns a refusal for the caller to show, and re-reads a stale list", async () => {
    const { bridge } = await startedApp();
    const actions = useAccountActions(() => "forge");
    const taken = await actions.create("PAUL", GOOD, "readonly");
    expect(taken).toEqual({ kind: "refused", refusal: { kind: "username_taken" } });
    const last = await actions.changeRole(
      {
        id: "SIMACCOUNT0001",
        username: "marie",
        role: "admin",
        createdAt: "",
        lastLoginAt: null,
        sessionsOpen: 1,
      },
      "readonly",
    );
    expect(last).toEqual({ kind: "refused", refusal: { kind: "last_admin" } });
    expect(bridge.calls.filter((c) => c === "account list")).toHaveLength(1);
  });

  it("announces the sessions closed and the role changed with the texts of the specification", async () => {
    await startedApp();
    const store = useAccountsStore();
    await store.load("forge");
    const toasts = useToastsStore();
    const actions = useAccountActions(() => "forge");
    const paul = store.of("forge")?.accounts.find((a) => a.username === "paul");
    expect(paul).toBeDefined();
    if (!paul) return;
    await actions.closeSessions(paul);
    await actions.changeRole(paul, "admin");
    const messages = toasts.items.map((t) => t.message);
    expect(messages).toContain("Sessions de paul fermées");
    expect(messages).toContain("paul est maintenant Administrateur");
  });

  it("an action cut before its answer is unknown, said once, never replayed; the list is re-read when the link is back", async () => {
    const { bridge } = await startedApp();
    const toasts = useToastsStore();
    const actions = useAccountActions(() => "forge");
    bridge.actionMode = "cut";
    const report = await actions.create("sophie", GOOD, "readonly");
    expect(report).toEqual({ kind: "unknown" });
    expect(bridge.calls.filter((c) => c === "account create sophie")).toHaveLength(1);
    expect(toasts.items).toHaveLength(1);
    expect(toasts.items[0]?.message).toContain("n'a pas été rejouée automatiquement");
  });

  it("is refused without anything sent while the link is not connected", async () => {
    const { bridge } = await startedApp();
    bridge.setState("forge", "offline");
    const toasts = useToastsStore();
    const actions = useAccountActions(() => "forge");
    const report = await actions.create("sophie", GOOD, "readonly");
    expect(report).toEqual({ kind: "failed" });
    expect(bridge.calls.some((c) => c.startsWith("account create"))).toBe(false);
    expect(toasts.items[0]?.kind).toBe("error");
  });
});

describe("refusal texts", () => {
  it("are the ones of the specification", () => {
    expect(refusalMessage({ kind: "last_admin" })).toBe(
      "Il doit toujours rester au moins un administrateur",
    );
    expect(refusalMessage({ kind: "last_admin" }, true)).toBe(
      "Tu es le dernier administrateur, ce compte ne peut pas être supprimé",
    );
    expect(refusalMessage({ kind: "forbidden" })).toBe(
      "Tu n'as pas la permission pour accéder à la gestion des comptes",
    );
    expect(refusalMessage({ kind: "wrong_password" })).toBe("L'ancien mot de passe est incorrect");
    expect(refusalMessage({ kind: "confirmation_mismatch" })).toBe(
      "L'identifiant ne correspond pas, réessaye",
    );
    expect(refusalMessage({ kind: "weak_password", rules: ["digit", "min_length"] })).toBe(
      "Le mot de passe doit contenir au moins un chiffre",
    );
    expect(refusalMessage({ kind: "invalid_username", problem: "too_short" })).toBe(
      "L'identifiant doit contenir au moins 3 caractères",
    );
  });
});
