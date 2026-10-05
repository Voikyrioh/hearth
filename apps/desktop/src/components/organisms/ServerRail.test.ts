import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";
import ServerRail from "./ServerRail.vue";

function mountRail() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: { template: "<div />" } },
      { path: "/settings", component: { template: "<div />" } },
    ],
  });
  return mount(ServerRail, { global: { plugins: [router] } });
}

describe("ServerRail", () => {
  it("is a labelled navigation with logo, add button and settings", () => {
    const wrapper = mountRail();
    expect(wrapper.get("nav").attributes("aria-label")).toBe("Serveurs");
    expect(wrapper.get('a[aria-label="Accueil"]').attributes("href")).toBe("/");
    expect(wrapper.get('a[aria-label="Réglages"]').attributes("href")).toBe("/settings");
  });

  it("keeps the add button inactive with an explanation until the wizard exists", () => {
    const add = mountRail().get('button[aria-label="Ajouter un serveur"]');
    expect(add.attributes("aria-disabled")).toBe("true");
    expect(add.attributes("title")).toBe("Bientôt disponible");
  });
});
