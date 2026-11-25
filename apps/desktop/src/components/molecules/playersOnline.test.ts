import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick } from "vue";
import { useOnlinePlayers } from "@/composables/useOnlinePlayers";
import type { PlayersView } from "@/link";
import { ageText, hasPlayersCard, playersShown } from "@/services/players";
import { freshBridge } from "@/test/app";
import PlayersOnlineCard from "./PlayersOnlineCard.vue";

// Les joueurs connectés (HRT-291, BR-CONSOLE-011) : la carte, ses phrases sans chiffre, la lecture toutes les 30 s.
const NOW = Date.parse("2026-11-18T20:45:24Z");

function view(patch: Partial<PlayersView> = {}): PlayersView {
  return {
    state: "ok",
    online: 3,
    max: 20,
    names: ["Alice_42", "Bram", "Chloé_m"],
    hiddenNames: 0,
    moreNames: 0,
    readAt: "2026-11-18T20:45:12Z",
    ...patch,
  };
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe("ce que la carte dit", () => {
  it("3 joueurs sur 20 places : le nombre, les places, les pseudos, l'âge de la lecture", () => {
    const wrapper = mount(PlayersOnlineCard, {
      props: { state: { kind: "ready", view: view() }, now: NOW, refreshing: false },
    });
    expect(wrapper.find("[data-players-count]").text()).toBe("3 sur 20 places");
    expect(wrapper.findAll("[data-players-names] li").map((li) => li.text())).toEqual([
      "Alice_42",
      "Bram",
      "Chloé_m",
    ]);
    expect(wrapper.find("[data-players-read]").text()).toContain("il y a 12 s");
    expect(wrapper.find("[data-players-read]").text()).toContain("toutes les 30 s");
  });

  it("personne : « Personne n'est connecté » avec les places", () => {
    const wrapper = mount(PlayersOnlineCard, {
      props: {
        state: { kind: "ready", view: view({ online: 0, names: [] }) },
        now: NOW,
        refreshing: false,
      },
    });
    expect(wrapper.find("[data-players-count]").text()).toBe("Personne n'est connecté 20 places");
  });

  it("des pseudos comptés sans être montrés, et des noms au-delà de cent, sont dits", () => {
    const wrapper = mount(PlayersOnlineCard, {
      props: {
        state: { kind: "ready", view: view({ hiddenNames: 2, moreNames: 5 }) },
        now: NOW,
        refreshing: false,
      },
    });
    expect(wrapper.find("[data-players-more]").text()).toBe("et 5 autres joueurs");
    expect(wrapper.find("[data-players-hidden]").text()).toBe("2 pseudos ne sont pas affichés.");
  });

  it("un pseudo est du texte : jamais interprété comme du HTML", () => {
    const wrapper = mount(PlayersOnlineCard, {
      props: {
        state: { kind: "ready", view: view({ names: ["<img src=x onerror=boom>"] }) },
        now: NOW,
        refreshing: false,
      },
    });
    expect(wrapper.find("[data-players-names] img").exists()).toBe(false);
    expect(wrapper.find("[data-players-names]").text()).toContain("<img");
  });

  it.each([
    ["stopped", "Personne n'est connecté : le serveur est arrêté."],
    ["starting", "Le nombre de joueurs s'affiche quand le serveur est prêt."],
    ["unavailable", "Hearth n'a pas pu lire les joueurs connectés."],
  ] as const)("l'état %s dit sa phrase, sans aucun chiffre", (state, sentence) => {
    const wrapper = mount(PlayersOnlineCard, {
      props: {
        state: {
          kind: "ready",
          view: { ...view(), state, online: null, max: null, names: [], readAt: null },
        },
        now: NOW,
        refreshing: false,
      },
    });
    expect(wrapper.find("[data-players-sentence]").text()).toBe(sentence);
    expect(wrapper.find("[data-players-count]").exists()).toBe(false);
  });

  it("un service sans console parlée n'a pas de carte", () => {
    const none = mount(PlayersOnlineCard, {
      props: {
        state: { kind: "ready", view: view({ state: "no_console", online: null }) },
        now: NOW,
        refreshing: false,
      },
    });
    expect(none.find("[data-players-card]").exists()).toBe(false);
    expect(hasPlayersCard(view({ state: "no_console" }))).toBe(false);
    const absent = mount(PlayersOnlineCard, {
      props: { state: { kind: "absent" }, now: NOW, refreshing: false },
    });
    expect(absent.find("[data-players-card]").exists()).toBe(false);
  });

  it("« relire » et le lien vers la console émettent", async () => {
    const wrapper = mount(PlayersOnlineCard, {
      props: {
        state: { kind: "ready", view: view() },
        now: NOW,
        refreshing: false,
        linkLabel: "Écrire aux joueurs dans la console",
      },
    });
    await wrapper.find("[data-players-refresh]").trigger("click");
    await wrapper.find("[data-players-console]").trigger("click");
    expect(wrapper.emitted("refresh")).toHaveLength(1);
    expect(wrapper.emitted("openConsole")).toHaveLength(1);
  });
});

describe("règles pures", () => {
  it("l'âge d'une lecture se dit en secondes puis en minutes, et une date illisible n'en dit rien", () => {
    expect(ageText("2026-11-18T20:45:12Z", NOW)).toBe("12 s");
    expect(ageText("2026-11-18T20:43:24Z", NOW)).toBe("2 min");
    expect(ageText(null, NOW)).toBeNull();
    expect(ageText("hier", NOW)).toBeNull();
  });

  it("un état « ok » sans chiffre ne devient jamais un nombre", () => {
    expect(playersShown(view({ online: null }))).toMatchObject({ kind: "sentence" });
  });
});

describe("la lecture des joueurs", () => {
  function host(key: () => string | null) {
    const { bridge } = freshBridge();
    bridge.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth" }]);
    bridge.console.seed("forge", "h_mc", {
      available: true,
      unavailableReason: null,
      state: "ready",
      canWrite: true,
      policy: { refused: [], sensitive: [] },
      quick: [],
      limits: null,
    });
    bridge.console.seedPlayers("forge", "h_mc", view());
    let players!: ReturnType<typeof useOnlinePlayers>;
    const wrapper = mount(
      defineComponent({
        setup() {
          players = useOnlinePlayers(() => "forge", key);
          return () => h("div");
        },
      }),
    );
    return { bridge, players, wrapper };
  }

  it("lit à l'ouverture, relit toutes les 30 s page ouverte, et plus du tout page fermée", async () => {
    const { bridge, wrapper, players } = host(() => "h_mc");
    await flushPromises();
    expect(players.state.value.kind).toBe("ready");
    expect(bridge.console.playersRead).toHaveLength(1);
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(bridge.console.playersRead).toHaveLength(2);
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(bridge.console.playersRead).toHaveLength(3);
    wrapper.unmount();
    vi.advanceTimersByTime(120_000);
    await flushPromises();
    expect(bridge.console.playersRead).toHaveLength(3);
  });

  it("sans clé (lien coupé, agent sans la capacité), rien n'est lu", async () => {
    const { bridge, players } = host(() => null);
    await flushPromises();
    await nextTick();
    expect(players.state.value.kind).toBe("idle");
    expect(bridge.console.playersRead).toHaveLength(0);
  });

  it("une relecture qui échoue garde la dernière vue lue, une première lecture qui échoue ne devine rien", async () => {
    const { bridge, players } = host(() => "h_mc");
    await flushPromises();
    bridge.console.failPlayers = true;
    await players.refresh();
    expect(players.state.value.kind).toBe("ready");
    const fresh = host(() => "h_mc");
    fresh.bridge.console.failPlayers = true;
    await flushPromises();
    expect(fresh.players.state.value.kind).toBe("failed");
  });

  it("« relire » force une lecture tout de suite", async () => {
    const { bridge, players } = host(() => "h_mc");
    await flushPromises();
    await players.refresh();
    expect(bridge.console.playersRead).toHaveLength(2);
  });
});
