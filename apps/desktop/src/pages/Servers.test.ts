import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SAMPLE_AGENT } from "@/link";
import { useToastsStore } from "@/stores/toasts";
import { mountContext } from "@/test/mount";
import Servers from "./Servers.vue";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

async function book() {
  const ctx = await mountContext({ agents: [SAMPLE_AGENT] });
  await ctx.router.push("/servers");
  const wrapper = mount(Servers, { global: ctx.global, attachTo: document.body });
  return { ...ctx, wrapper };
}

function row(wrapper: ReturnType<typeof mount>, id: string) {
  return wrapper.get(`[data-server-row="${id}"]`);
}

function button(
  scope: {
    findAll: (s: string) => Array<{ text: () => string; trigger: (e: string) => Promise<void> }>;
  },
  label: string,
) {
  const found = scope.findAll("button").find((b) => b.text() === label);
  if (!found) throw new Error(`bouton « ${label} » introuvable`);
  return found;
}

describe("server book", () => {
  it("lists every server with its own state and the actions that fit it", async () => {
    const { wrapper, bridge } = await book();
    expect(wrapper.get("h1").text()).toBe("Mes serveurs");
    expect(button(wrapper, "Ajouter un serveur")).toBeDefined();
    const forge = row(wrapper, "forge");
    expect(forge.text()).toContain("forge");
    expect(forge.text()).toContain("192.168.1.120");
    expect(forge.text()).toContain("Identifiants mémorisés");
    expect(forge.get("[role=status]").text()).toBe("Connecté");
    const labels = forge.findAll("button").map((b) => b.text());
    expect(labels).toEqual(["Se déconnecter", "Oublier mes identifiants", "Modifier", "Supprimer"]);
    // Un serveur dont le mot de passe n'est pas mémorisé n'offre pas l'oubli.
    const salon = row(wrapper, "salon");
    expect(salon.text()).not.toContain("Identifiants mémorisés");
    expect(salon.findAll("button").map((b) => b.text())).not.toContain("Oublier mes identifiants");
    // Chaque serveur a son état : l'un hors ligne, l'autre reste connecté (BR-CONN-015).
    bridge.setState("salon", "offline");
    await flushPromises();
    expect(row(wrapper, "salon").get("[role=status]").text()).toBe("Hors ligne");
    expect(row(wrapper, "forge").get("[role=status]").text()).toBe("Connecté");
  });

  it("disconnects one server without touching the others, keeping its remembered password", async () => {
    const { wrapper, bridge, servers } = await book();
    bridge.vault.set("forge", "Correct-Horse-9");
    await button(row(wrapper, "forge"), "Se déconnecter").trigger("click");
    await flushPromises();
    expect(row(wrapper, "forge").get("[role=status]").text()).toBe("Session expirée");
    expect(row(wrapper, "salon").get("[role=status]").text()).toBe("Connecté");
    expect(bridge.vault.get("forge")).toBe("Correct-Horse-9");
    expect(servers.byId("forge")?.remember).toBe(true);
  });

  it("forgets the remembered credentials and says so", async () => {
    const { wrapper, bridge, servers } = await book();
    bridge.vault.set("forge", "Correct-Horse-9");
    await button(row(wrapper, "forge"), "Oublier mes identifiants").trigger("click");
    await flushPromises();
    expect(bridge.vault.has("forge")).toBe(false);
    expect(servers.byId("forge")?.remember).toBe(false);
    expect(useToastsStore().items.at(-1)?.message).toBe("Identifiants oubliés pour forge.");
    expect(row(wrapper, "forge").text()).not.toContain("Identifiants mémorisés");
    // La session reste ouverte.
    expect(row(wrapper, "forge").get("[role=status]").text()).toBe("Connecté");
  });

  it("asks for a confirmation before removing, and wipes the secrets with the server", async () => {
    const { wrapper, bridge, servers } = await book();
    bridge.vault.set("forge", "Correct-Horse-9");
    await button(row(wrapper, "forge"), "Supprimer").trigger("click");
    await flushPromises();
    const dialog = document.body.querySelector("dialog");
    expect(dialog?.textContent).toContain("Supprimer ce serveur ?");
    expect(dialog?.textContent).toContain("Ses identifiants mémorisés seront aussi supprimés.");
    const buttons = [...(dialog?.querySelectorAll("button") ?? [])];
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(["Annuler", "Supprimer"]);
    // Annuler : rien ne change.
    buttons[0]?.click();
    await flushPromises();
    expect(servers.servers.map((s) => s.id)).toEqual(["forge", "salon"]);
    // Confirmer : le serveur et ses secrets partent.
    await button(row(wrapper, "forge"), "Supprimer").trigger("click");
    await flushPromises();
    [...document.body.querySelectorAll<HTMLButtonElement>("dialog button")][1]?.click();
    await flushPromises();
    expect(servers.servers.map((s) => s.id)).toEqual(["salon"]);
    expect(bridge.vault.has("forge")).toBe(false);
  });

  it("renames and recolors a server in place, and refuses a name already used", async () => {
    const { wrapper, servers } = await book();
    await button(row(wrapper, "forge"), "Modifier").trigger("click");
    const form = wrapper.get("section form");
    const name = form.findAll("input")[0];
    await name?.setValue("NAS-Salon");
    expect(form.text()).toContain("Un serveur porte déjà ce nom");
    expect(
      form
        .findAll("button")
        .find((b) => b.text() === "Enregistrer")
        ?.attributes("aria-disabled"),
    ).toBe("true");
    await name?.setValue("Cave");
    await form.findAll("[role=radio]")[4]?.trigger("click");
    await form.trigger("submit");
    await flushPromises();
    expect(servers.byId("forge")).toMatchObject({ name: "Cave", color: 5 });
    expect(wrapper.find("section form").exists()).toBe(false);
    expect(row(wrapper, "forge").text()).toContain("Cave");
  });

  it("asks to verify the fingerprint again when the address changes, and saves only once confirmed", async () => {
    const { wrapper, servers, bridge } = await book();
    await button(row(wrapper, "forge"), "Modifier").trigger("click");
    const form = wrapper.get("section form");
    await form.findAll("input")[1]?.setValue("192.168.1.50");
    expect(form.text()).toContain("l'identité du serveur doit être vérifiée de nouveau");
    await form.trigger("submit");
    await flushPromises();
    expect(wrapper.get("section").text()).toContain("Vérifie l'identité du serveur");
    expect(wrapper.findAll("[data-fingerprint] span")).toHaveLength(8);
    // Refuser : retour au formulaire, rien n'est modifié.
    await button(wrapper.get("section"), "Refuser").trigger("click");
    expect(servers.byId("forge")?.host).toBe("192.168.1.120");
    expect(bridge.calls.some((call) => call.startsWith("update"))).toBe(false);
    await wrapper.get("section form").trigger("submit");
    await flushPromises();
    await button(wrapper.get("section"), "Confirmer").trigger("click");
    await flushPromises();
    expect(servers.byId("forge")).toMatchObject({ host: "192.168.1.50", remember: true });
  });

  it("tells the user when the new address cannot be reached, and changes nothing", async () => {
    const { wrapper, servers } = await book();
    await button(row(wrapper, "forge"), "Modifier").trigger("click");
    const form = wrapper.get("section form");
    await form.findAll("input")[1]?.setValue("10.9.9.9");
    await form.trigger("submit");
    await flushPromises();
    expect(wrapper.get("section [role=alert]").text()).toBe(
      "Cette adresse n'est pas joignable. Vérifie l'adresse et essaie de nouveau.",
    );
    expect(servers.byId("forge")?.host).toBe("192.168.1.120");
  });
});
