import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";
import Welcome from "./Welcome.vue";

const stub = { template: "<div />" };

async function mountWelcome() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/welcome", name: "welcome", component: Welcome },
      { path: "/servers/new", name: "add-server", component: stub },
    ],
  });
  await router.push("/welcome");
  return { wrapper: mount(Welcome, { global: { plugins: [router] } }), router };
}

describe("Welcome (BR-CLIENT-013)", () => {
  it("shows the exact first-launch texts", async () => {
    const { wrapper } = await mountWelcome();
    expect(wrapper.get("h1").text()).toBe("Bienvenue dans Hearth");
    expect(wrapper.text()).toContain("Ajoute ton premier serveur pour commencer.");
    expect(wrapper.get("button").text()).toBe("Ajouter un serveur");
  });

  it("draws the decorative first-launch illustration", async () => {
    const { wrapper } = await mountWelcome();
    const img = wrapper.get("img");
    expect(img.attributes("alt")).toBe("");
    expect(img.attributes("src")).toContain("vide-premier-lancement");
  });

  it("opens the add-server wizard from the main button", async () => {
    const { wrapper, router } = await mountWelcome();
    expect(wrapper.get("button").attributes("aria-disabled")).toBeUndefined();
    await wrapper.get("button").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("add-server");
  });
});
