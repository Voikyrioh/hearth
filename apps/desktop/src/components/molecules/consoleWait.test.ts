import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ConsoleWaitSection from "./ConsoleWaitSection.vue";

// « Ce qui attend la console » (HRT-269, BR-FICHE-051 amendée) : quand l'agent parle à la console du jeu, la section ne garde
// que ce qui attend vraiment et dit où trouver le reste ; les autres jeux la gardent inchangée.
describe("la section « Ce qui attend la console »", () => {
  it("sans console parlée : les trois fonctions, inchangées", () => {
    const wrapper = mount(ConsoleWaitSection);
    expect(
      wrapper
        .findAll("[data-console-function]")
        .map((el) => el.attributes("data-console-function")),
    ).toEqual(["apply", "who", "tell"]);
    expect(wrapper.find("[data-console-served]").exists()).toBe(false);
  });

  it("avec la console parlée : seulement « Appliquer un réglage sans redémarrer », et « Ouvrir la console »", async () => {
    const wrapper = mount(ConsoleWaitSection, { props: { served: true } });
    expect(
      wrapper
        .findAll("[data-console-function]")
        .map((el) => el.attributes("data-console-function")),
    ).toEqual(["apply"]);
    expect(wrapper.get("[data-console-function='apply']").text()).toContain(
      "Cette fonction attend la console de Hearth, qui n'est pas encore livrée.",
    );
    expect(wrapper.get("[data-console-served]").text()).toContain("onglet Console");
    await wrapper.get("[data-console-open]").trigger("click");
    expect(wrapper.emitted("openConsole")).toHaveLength(1);
  });
});
