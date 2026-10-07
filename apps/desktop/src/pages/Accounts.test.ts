import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "@/App.vue";
import { SCREEN_ILLUSTRATIONS } from "@/assets/illustrations/screens";
import { LinkCommandError } from "@/link";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

afterEach(() => {
  document.body.innerHTML = "";
});

async function boot(path = "/servers/forge/accounts") {
  const ctx = await mountContext();
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

const rows = (wrapper: VueWrapper) =>
  wrapper.findAll("tbody tr").map((row) => row.attributes("data-account"));

const GOOD = "Sunny-Walk-Home-42";

describe("Comptes (administrateurs)", () => {
  it("shows the list of the agent in the five columns", async () => {
    const { wrapper } = await boot();
    expect(wrapper.get("h1").text()).toBe("Comptes");
    expect(rows(wrapper)).toEqual(["marie", "paul", "lea"]);
    expect(wrapper.get('[data-account="paul"]').text()).toContain("2");
    wrapper.unmount();
  });

  it("has no « Comptes » entry in the menu of a read-only account, and the route stays shut", async () => {
    const { wrapper, router } = await boot("/servers/salon/dashboard");
    const entries = wrapper.findAll(".nav__item").map((e) => e.text());
    expect(entries).toEqual(["Tableau de bord", "Sécurité"]);
    await router.push("/servers/salon/accounts");
    expect(router.currentRoute.value.fullPath).toBe("/servers/salon/dashboard");
    wrapper.unmount();
  });

  it("shows the refusal of the agent when a read-only account forces the list, without a list", async () => {
    const ctx = await mountContext();
    vi.spyOn(ctx.bridge, "listAccounts").mockRejectedValue(
      new LinkCommandError({ kind: "forbidden" }),
    );
    await ctx.router.push("/servers/forge/accounts");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe(
      "Tu n'as pas la permission pour accéder à la gestion des comptes",
    );
    expect(wrapper.find("table").exists()).toBe(false);
    wrapper.unmount();
  });

  it("shows the empty state of the specification with a button to add one", async () => {
    const ctx = await mountContext();
    ctx.bridge.accounts.seed("forge", []);
    await ctx.router.push("/servers/forge/accounts");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    expect(wrapper.text()).toContain(
      "Aucun compte n'existe pour le moment. Crée-en un pour commencer.",
    );
    expect(wrapper.findAll("button").filter((b) => b.text() === "Ajouter un compte")).toHaveLength(
      2,
    );
    // Illustration : celle de la table des écrans (aucune tant que `accounts` y vaut null).
    expect(wrapper.findAll(".empty__img")).toHaveLength(SCREEN_ILLUSTRATIONS.accounts ? 1 : 0);
    wrapper.unmount();
  });

  it("creates an account from the header button and adds it to the list", async () => {
    const { wrapper, bridge } = await boot();
    await wrapper.get("#header-actions button").trigger("click");
    await flushPromises();
    typeInto(document.querySelector('input[placeholder="ton-identifiant"]'), "sophie");
    typeInto(document.querySelector('input[placeholder="Mot de passe"]'), GOOD);
    typeInto(document.querySelector('input[placeholder="Confirme le mot de passe"]'), GOOD);
    await flushPromises();
    await confirmDialog("Créer");
    expect(rows(wrapper)).toEqual(["marie", "paul", "lea", "sophie"]);
    expect(useToastsStore().items.map((t) => t.message)).toContain("Compte sophie créé");
    expect(JSON.stringify(bridge.calls)).not.toContain(GOOD);
    wrapper.unmount();
  });

  it("closes the sessions of an account after a confirmation that names it, and says so", async () => {
    const { wrapper } = await boot();
    const close = wrapper.get('[data-account="paul"]').findAll("button")[2];
    expect(close?.text()).toBe("Fermer les 2 sessions");
    await close?.trigger("click");
    await flushPromises();
    expect(document.body.textContent).toContain("Fermer les sessions de paul ?");
    await confirmDialog("Fermer les sessions");
    expect(useToastsStore().items.map((t) => t.message)).toContain("Sessions de paul fermées");
    expect(wrapper.get('[data-account="paul"] td:nth-child(5)').text()).toBe("0");
    expect(
      wrapper.get('[data-account="paul"]').findAll("button")[2]?.attributes("aria-disabled"),
    ).toBe("true");
    wrapper.unmount();
  });

  it("changes a role from the dropdown and announces it", async () => {
    const { wrapper } = await boot();
    const paul = wrapper.get('[data-account="paul"]');
    await paul.findAll("button")[0]?.trigger("click");
    await flushPromises();
    await paul.get("select").setValue("admin");
    await flushPromises();
    expect(document.body.textContent).toContain("Changer le rôle de paul ?");
    await confirmDialog("Changer le rôle");
    expect(useToastsStore().items.map((t) => t.message)).toContain(
      "paul est maintenant Administrateur",
    );
    expect(wrapper.get('[data-account="paul"] td:nth-child(2)').text()).toBe("Administrateur");
    wrapper.unmount();
  });

  it("asks for a confirmation that names the account before deleting it", async () => {
    const { wrapper, bridge } = await boot();
    await wrapper.get('[data-account="lea"]').findAll("button")[3]?.trigger("click");
    await flushPromises();
    expect(document.body.textContent).toContain("Supprimer le compte ?");
    expect(document.body.textContent).toContain(
      "Supprimer le compte lea ? Cette action est irréversible.",
    );
    // Annuler : rien n'est parti.
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(bridge.calls.some((c) => c.startsWith("account delete"))).toBe(false);
    await wrapper.get('[data-account="lea"]').findAll("button")[3]?.trigger("click");
    await confirmDialog("Supprimer");
    expect(rows(wrapper)).toEqual(["marie", "paul"]);
    expect(useToastsStore().items.map((t) => t.message)).toContain("Compte lea supprimé");
    wrapper.unmount();
  });

  it("explains the last administrator when the agent refuses a removal, and leaves the account", async () => {
    const ctx = await mountContext();
    ctx.bridge.accounts.seed("forge", [
      { id: "A", username: "marie", role: "admin" },
      { id: "J", username: "jean", role: "admin" },
    ]);
    await ctx.router.push("/servers/forge/accounts");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    // Entre-temps, l'agent a rétrogradé marie : l'écran, lui, montre encore deux administrateurs.
    ctx.bridge.accounts.seed("forge", [
      { id: "A", username: "marie", role: "readonly" },
      { id: "J", username: "jean", role: "admin" },
    ]);
    await wrapper.get('[data-account="jean"]').findAll("button")[3]?.trigger("click");
    await confirmDialog("Supprimer");
    expect(useToastsStore().items.map((t) => t.message)).toContain(
      "Il doit toujours rester au moins un administrateur",
    );
    expect(document.querySelector("dialog")).toBeNull();
    expect(rows(wrapper)).toEqual(["marie", "jean"]);
    // La liste a été relue : jean est maintenant le seul administrateur, ses boutons sont grisés.
    expect(
      wrapper.get('[data-account="jean"]').findAll("button")[3]?.attributes("aria-disabled"),
    ).toBe("true");
    wrapper.unmount();
  });

  it("an action cut before its answer is unknown, said once, not replayed, and the list is re-read when the link is back", async () => {
    const { wrapper, bridge } = await boot();
    bridge.actionMode = "cut";
    await wrapper.get('[data-account="lea"]').findAll("button")[3]?.trigger("click");
    await confirmDialog("Supprimer");
    const messages = useToastsStore().items.map((t) => t.message);
    expect(messages).toContain(
      "Le résultat de cette opération n'est pas connu. Elle n'a pas été rejouée automatiquement. À la reconnexion, la liste se mettra à jour.",
    );
    expect(bridge.calls.filter((c) => c.startsWith("account delete"))).toHaveLength(1);
    // Hors lien : la liste affichée est celle d'avant (désaturée par le gabarit).
    expect(rows(wrapper)).toContain("lea");
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(rows(wrapper)).not.toContain("lea");
    // Toujours un seul envoi.
    expect(bridge.calls.filter((c) => c.startsWith("account delete"))).toHaveLength(1);
    wrapper.unmount();
  });

  it("disables every action with its explanation while the server is offline", async () => {
    const { wrapper, bridge } = await boot();
    bridge.setState("forge", "offline");
    await flushPromises();
    const buttons = wrapper.findAll("tbody button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const b of buttons) expect(b.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("#header-actions button").attributes("aria-disabled")).toBe("true");
    // La liste d'avant reste affichée.
    expect(rows(wrapper)).toEqual(["marie", "paul", "lea"]);
    wrapper.unmount();
  });
});
