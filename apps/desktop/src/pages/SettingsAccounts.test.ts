import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { mountContext } from "@/test/mount";

afterEach(() => {
  document.body.innerHTML = "";
});

async function boot() {
  const ctx = await mountContext();
  await ctx.router.push("/settings");
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

describe("Réglages : l'ordre du design et la section personnelle", () => {
  it("puts the sections in the order of the design: Général, Client, Mises à jour, then Mon compte on the right", async () => {
    const { wrapper } = await boot();
    expect(wrapper.findAll("h2").map((h) => h.text())).toEqual([
      "Général",
      "Client",
      "Mises à jour",
      "Mon compte",
    ]);
    // Les notifications et la version sont des réglages du client.
    const client = wrapper.findAll(".settings__panel")[1];
    expect(client?.text()).toContain("Notifier quand un serveur devient hors ligne ou revient");
    expect(client?.text()).toContain("Version");
    wrapper.unmount();
  });

  it("offers « Changer mon mot de passe » on every server, whatever the role, and the removal to administrators only", async () => {
    const { wrapper } = await boot();
    const cards = wrapper.findAll(".mine");
    expect(cards.map((c) => c.attributes("data-server"))).toEqual(["forge", "salon"]);
    const forge = cards[0];
    const salon = cards[1];
    expect(forge?.findAll("button").map((b) => b.text())).toEqual([
      "Changer mon mot de passe",
      "Supprimer mon compte",
    ]);
    expect(salon?.findAll("button").map((b) => b.text())).toEqual(["Changer mon mot de passe"]);
    wrapper.unmount();
  });

  it("tells why a card's button is unavailable when THAT server is offline, not the displayed one", async () => {
    const { wrapper, bridge } = await boot();
    bridge.setState("salon", "offline");
    await flushPromises();
    const salon = wrapper.get('.mine[data-server="salon"] button');
    const forge = wrapper.get('.mine[data-server="forge"] button');
    expect(salon.attributes("aria-disabled")).toBe("true");
    expect(forge.attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });
});
