import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { startedApp } from "@/test/app";
import HButton from "./HButton.vue";
import HToggle from "./HToggle.vue";

describe("HButton", () => {
  it("emits click when active", async () => {
    const wrapper = mount(HButton, { slots: { default: "Valider" } });
    await wrapper.get("button").trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
    expect(wrapper.get("button").attributes("aria-disabled")).toBeUndefined();
  });

  it("stays focusable, explains itself with a tooltip and ignores clicks when disabled", async () => {
    const wrapper = mount(HButton, {
      props: { disabled: true, hint: "Bientôt disponible" },
      slots: { default: "Valider" },
    });
    const button = wrapper.get("button");
    await button.trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(button.attributes("disabled")).toBeUndefined();
    expect(button.attributes("title")).toBeUndefined();
    expect(wrapper.get('[role="tooltip"]').text()).toBe("Bientôt disponible");
    expect(button.attributes("aria-describedby")).toBe(
      wrapper.get('[role="tooltip"]').attributes("id"),
    );
  });

  it("shows the explanation on keyboard focus, not only on hover", async () => {
    const wrapper = mount(HButton, { props: { disabled: true, hint: "Pourquoi" } });
    const bubble = () => wrapper.get('[role="tooltip"]').element as HTMLElement;
    expect(bubble().style.display).toBe("none");
    await wrapper.get("button").trigger("focusin");
    expect(bubble().style.display).not.toBe("none");
  });

  it("applies the variant class and passes its attributes to the button", () => {
    const wrapper = mount(HButton, {
      props: { variant: "secondary" },
      attrs: { "aria-label": "Nom", "data-x": "1" },
    });
    const button = wrapper.get("button");
    expect(button.classes()).toContain("btn--secondary");
    expect(button.attributes("aria-label")).toBe("Nom");
    expect(button.attributes("data-x")).toBe("1");
  });
});

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

// Un seul écrivain : `disabled`, `busy` et le lien se combinent dans le composant. Le bug
// d'origine (le lien tombe pendant la requête, `busy` repasse à faux, l'aspect actif revient
// alors que le serveur manque) ne peut plus arriver.
describe("needs-link on HButton and HToggle (single source of truth)", () => {
  async function setup() {
    const ctx = await startedApp();
    ctx.servers.setCurrent("forge");
    return ctx;
  }

  it("is inert and explained while the link is down, active again on return", async () => {
    const { bridge, pinia } = await setup();
    const wrapper = mount(HButton, { props: { needsLink: true }, global: { plugins: [pinia] } });
    const button = wrapper.get("button");
    await button.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Indisponible tant que le serveur est hors ligne.",
    );
    await button.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(1);
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(button.attributes("aria-disabled")).toBeUndefined();
    expect(wrapper.find('[role="tooltip"]').exists()).toBe(false);
    await button.trigger("click");
    expect(wrapper.emitted("click")).toHaveLength(2);
  });

  it("stays inert when busy ends DURING a link cut (the original bug)", async () => {
    const { bridge, pinia } = await setup();
    const wrapper = mount(HButton, {
      props: { needsLink: true, busy: true },
      global: { plugins: [pinia] },
    });
    bridge.setState("forge", "offline");
    await flushPromises();
    await wrapper.setProps({ busy: false });
    const button = wrapper.get("button");
    expect(button.attributes("aria-disabled")).toBe("true");
    await button.trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
    // Retour du lien pendant que le bouton est de nouveau occupé : toujours inerte.
    bridge.setState("forge", "connected");
    await wrapper.setProps({ busy: true });
    await flushPromises();
    expect(button.attributes("aria-disabled")).toBe("true");
    await wrapper.setProps({ busy: false });
    expect(button.attributes("aria-disabled")).toBeUndefined();
  });

  it("combines disabled, busy and the link, whatever changes first", async () => {
    const { bridge, pinia } = await setup();
    const wrapper = mount(HButton, {
      props: { needsLink: true, disabled: true },
      global: { plugins: [pinia] },
    });
    const button = wrapper.get("button");
    bridge.setState("forge", "reconnecting");
    await flushPromises();
    await wrapper.setProps({ disabled: false });
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Indisponible pendant la reconnexion au serveur.",
    );
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(button.attributes("aria-disabled")).toBeUndefined();
  });

  it("keeps the keyboard focus and shows the reason on focus", async () => {
    const { bridge, pinia } = await setup();
    const wrapper = mount(HButton, {
      props: { needsLink: true },
      global: { plugins: [pinia] },
      attachTo: document.body,
    });
    bridge.setState("forge", "session_expired");
    await flushPromises();
    const button = wrapper.get("button");
    (button.element as HTMLElement).focus();
    expect(document.activeElement).toBe(button.element);
    await button.trigger("focusin");
    const bubble = wrapper.get('[role="tooltip"]');
    expect((bubble.element as HTMLElement).style.display).not.toBe("none");
    expect(bubble.text()).toBe("Indisponible : ta session a expiré.");
    // Entrée / Espace natifs déclenchent un clic : bloqué comme à la souris.
    await button.trigger("click");
    expect(wrapper.emitted("click")).toBeUndefined();
    wrapper.unmount();
  });

  it("requires the administrator role when asked, even with a connected link", async () => {
    const ctx = await setup();
    ctx.servers.setCurrent("salon");
    const wrapper = mount(HButton, {
      props: { needsLink: { role: "admin" } },
      global: { plugins: [ctx.pinia] },
    });
    expect(wrapper.get("button").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe("Réservé aux administrateurs.");
    ctx.servers.setCurrent("forge");
    await flushPromises();
    expect(wrapper.get("button").attributes("aria-disabled")).toBeUndefined();
  });

  it("explains itself when there is no current server, and ignores the other servers' links", async () => {
    const ctx = await setup();
    const wrapper = mount(HButton, {
      props: { needsLink: true },
      global: { plugins: [ctx.pinia] },
    });
    ctx.bridge.setState("salon", "offline");
    await flushPromises();
    expect(wrapper.get("button").attributes("aria-disabled")).toBeUndefined();
    ctx.servers.setCurrent(null);
    await flushPromises();
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Indisponible : aucun serveur sélectionné.",
    );
  });

  it("applies to HToggle with the same rules (disabled, busy, link combined)", async () => {
    const { bridge, pinia } = await setup();
    const wrapper = mount(HToggle, {
      props: { modelValue: false, needsLink: true, busy: true },
      global: { plugins: [pinia] },
    });
    const toggle = wrapper.get("button");
    bridge.setState("forge", "offline");
    await wrapper.setProps({ busy: false });
    await flushPromises();
    expect(toggle.attributes("aria-disabled")).toBe("true");
    await toggle.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toBeUndefined();
    expect(wrapper.get('[role="tooltip"]').text()).toContain("hors ligne");
    bridge.setState("forge", "connected");
    await flushPromises();
    await toggle.trigger("click");
    expect(wrapper.emitted("update:modelValue")).toEqual([[true]]);
  });
});
