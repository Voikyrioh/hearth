import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import { useGameSettingsPendingStore } from "./gameSettingsPending";

describe("les réglages en attente d'application (BR-FICHE-042)", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("se souvient d'un redémarrage à faire, jamais d'un effet nul", () => {
    const store = useGameSettingsPendingStore();
    store.note("forge", "h_A", "none");
    expect(store.of("forge", "h_A")).toBeUndefined();
    store.note("forge", "h_A", "restart");
    expect(store.of("forge", "h_A")).toEqual({ effect: "restart" });
  });

  it("oublie le repère d'un service quand un démarrage est lancé, sans toucher aux autres", () => {
    const store = useGameSettingsPendingStore();
    store.note("forge", "h_A", "restart");
    store.note("forge", "h_B", "restart");
    store.clear("forge", "h_A");
    expect(store.of("forge", "h_A")).toBeUndefined();
    expect(store.of("forge", "h_B")).toBeDefined();
  });
});
