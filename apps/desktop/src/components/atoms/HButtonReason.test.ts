import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { startedApp } from "@/test/app";
import HButton from "./HButton.vue";

// HRT-40 (C42) : quand la raison est écrite sous le bouton, elle n'est dite qu'une fois (revue de la PR #59) :
// pas d'infobulle redondante, le texte visible décrit le bouton.
describe("HButton reason-below", () => {
  const props = { disabled: true, hint: "Seul un administrateur peut le faire" };

  it("écrit la raison sous le bouton, sans infobulle, et la lie au bouton", () => {
    const wrapper = mount(HButton, {
      props: { ...props, reasonBelow: true },
      slots: { default: "Agir" },
    });
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    const reason = wrapper.get("[data-button-reason]");
    expect(reason.attributes("id")).toBeTruthy();
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(reason.attributes("id"));
    expect(wrapper.text().split(props.hint).length - 1).toBe(1);
  });

  it("sans reason-below, la raison reste une infobulle décrivant le bouton", () => {
    const wrapper = mount(HButton, { props, slots: { default: "Agir" } });
    const bubble = wrapper.get('[role="tooltip"]');
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(bubble.attributes("id"));
    expect(wrapper.find("[data-button-reason]").exists()).toBe(false);
  });
});

// C4 de la review (HRT-78, BR-GRANT-014) : `describedBy` (la raison du droit écrite ailleurs) s'AJOUTE aux autres
// raisons du bouton, il ne les remplace pas. Un bouton bloqué pour deux raisons les garde toutes les deux.
describe("HButton describedBy", () => {
  it("seul : le bouton désactivé est décrit par ce texte, sans infobulle en plus", () => {
    const wrapper = mount(HButton, {
      props: { disabled: true, describedBy: "why" },
      slots: { default: "Démarrer" },
    });
    expect(wrapper.get("button").attributes("aria-describedby")).toBe("why");
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
  });

  it("sans blocage du droit (non désactivé), le texte ne décrit pas le bouton", () => {
    const wrapper = mount(HButton, {
      props: { describedBy: "why" },
      slots: { default: "Démarrer" },
    });
    expect(wrapper.get("button").attributes("aria-describedby")).toBeUndefined();
  });

  it("lien coupé, raison écrite dessous : les deux identifiants, celui du lien d'abord", async () => {
    const ctx = await startedApp();
    ctx.bridge.setState("forge", "offline");
    ctx.servers.setCurrent("forge");
    const wrapper = mount(HButton, {
      props: { disabled: true, needsLink: true, reasonBelow: true, describedBy: "why" },
      slots: { default: "Démarrer" },
      global: { plugins: [ctx.pinia] },
    });
    await flushPromises();
    const reasonId = wrapper.get("[data-button-reason]").attributes("id");
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(`${reasonId} why`);
  });

  it("lien coupé ET droit manquant : l'infobulle du lien et le texte du droit décrivent le bouton", async () => {
    const ctx = await startedApp();
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    ctx.servers.setCurrent("forge");
    const wrapper = mount(HButton, {
      props: { disabled: true, needsLink: true, describedBy: "why" },
      slots: { default: "Démarrer" },
      global: { plugins: [ctx.pinia] },
    });
    await flushPromises();
    const bubble = wrapper.get('[role="tooltip"]');
    expect(bubble.text()).toContain("Indisponible");
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(
      `${bubble.attributes("id")} why`,
    );
  });

  it("lien coupé seul : l'infobulle du lien, rien d'autre", async () => {
    const ctx = await startedApp();
    ctx.bridge.setState("forge", "offline");
    ctx.servers.setCurrent("forge");
    const wrapper = mount(HButton, {
      props: { needsLink: true },
      slots: { default: "Démarrer" },
      global: { plugins: [ctx.pinia] },
    });
    await flushPromises();
    const bubble = wrapper.get('[role="tooltip"]');
    expect(wrapper.get("button").attributes("aria-describedby")).toBe(bubble.attributes("id"));
  });
});
