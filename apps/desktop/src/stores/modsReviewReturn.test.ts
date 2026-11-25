import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import { useModsReviewReturnStore } from "./modsReviewReturn";

describe("le retour vers « Vérifiez avant d'appliquer »", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("se souvient du service d'où la console a été ouverte, et le rend une seule fois", () => {
    const store = useModsReviewReturnStore();
    expect(store.has("forge", "h_mc")).toBe(false);
    store.mark("forge", "h_mc");
    expect(store.has("forge", "h_mc")).toBe(true);
    expect(store.has("forge", "h_autre")).toBe(false);
    expect(store.take("forge", "h_mc")).toBe(true);
    expect(store.take("forge", "h_mc")).toBe(false);
  });

  it("l'oublie quand on quitte le parcours mods", () => {
    const store = useModsReviewReturnStore();
    store.mark("forge", "h_mc");
    store.forget("forge", "h_mc");
    expect(store.has("forge", "h_mc")).toBe(false);
  });

  it("chaque Pinia a le sien : rien ne fuit d'un montage à l'autre", () => {
    useModsReviewReturnStore().mark("forge", "h_mc");
    setActivePinia(createPinia());
    expect(useModsReviewReturnStore().has("forge", "h_mc")).toBe(false);
  });
});
