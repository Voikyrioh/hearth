import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { SAMPLE_SERVERS } from "@/link";
import { mountContext } from "@/test/mount";

// Review de la PR #19 : « moi » est l'identité que l'AGENT donne pour la session, jamais une
// comparaison de texte avec ce que l'utilisateur a tapé ; changer son mot de passe ne demande jamais
// la liste ; aucun critère n'est « Respecté » tant que rien n'est saisi.
afterEach(() => {
  document.body.innerHTML = "";
});

const GOOD = "Sunny-Walk-Home-42";

function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("« moi » vient de l'agent", () => {
  async function bootAsCapitals() {
    // Connecté en « Marie » : le carnet garde la saisie, l'agent a « marie ».
    const forge = { ...(SAMPLE_SERVERS[0] ?? SAMPLE_SERVERS[0]), username: "Marie" } as never;
    const ctx = await mountContext({ servers: [forge] });
    await ctx.router.push("/servers/forge/accounts");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    return { ...ctx, wrapper };
  }

  it("a login typed in capitals still shows « toi » and only « Changer mon mot de passe » on one's own line", async () => {
    const { wrapper } = await bootAsCapitals();
    const own = wrapper.get('[data-account="marie"]');
    expect(own.text()).toContain("toi");
    expect(own.findAll("button").map((b) => b.text())).toEqual(["Changer mon mot de passe"]);
    // Les autres lignes gardent leurs quatre actions.
    expect(wrapper.get('[data-account="paul"]').findAll("button")).toHaveLength(4);
    wrapper.unmount();
  });

  it("the settings can delete one's own account by its agent id, whatever case was typed", async () => {
    const forge = { ...SAMPLE_SERVERS[0], username: "Marie" } as never;
    const ctx = await mountContext({ servers: [forge] });
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await wrapper
      .findAll(".mine button")
      .find((b) => b.text() === "Supprimer mon compte")
      ?.trigger("click");
    await flushPromises();
    typeInto(document.querySelector('input[placeholder="ton-identifiant"]'), "marie");
    await flushPromises();
    document.querySelector<HTMLButtonElement>("dialog button[type=submit]")?.click();
    await flushPromises();
    // Le compte est bien trouvé (pas « Ce compte n'existe pas ») : l'agent répond « dernier administrateur ».
    expect(document.body.textContent).not.toContain("Ce compte n'existe pas");
    expect(document.body.textContent).toContain(
      "Tu es le dernier administrateur, ce compte ne peut pas être supprimé",
    );
    wrapper.unmount();
  });
});

describe("changer son mot de passe ne demande jamais la liste des comptes", () => {
  it("a read-only account changing its own password sends no « account list »", async () => {
    const ctx = await mountContext();
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await wrapper.get('.mine[data-server="salon"] button').trigger("click");
    await flushPromises();
    typeInto(document.querySelector('input[placeholder="Ancien mot de passe"]'), "Correct-Horse-9");
    typeInto(document.querySelector('input[placeholder="Nouveau mot de passe"]'), GOOD);
    typeInto(document.querySelector('input[placeholder="Confirme le mot de passe"]'), GOOD);
    await flushPromises();
    document.querySelector<HTMLButtonElement>("dialog button[type=submit]")?.click();
    await flushPromises();
    expect(ctx.bridge.calls).toContain("account own-password");
    expect(ctx.bridge.calls).not.toContain("account list");
    wrapper.unmount();
  });
});
