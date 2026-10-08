import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { LinkCommandError } from "@/link";
import { useSecurityStore } from "@/stores/security";
import { useToastsStore } from "@/stores/toasts";
import { mountContext } from "@/test/mount";

// HRT-26 : l'alerte, le mode attaque et l'état de ce poste, sur le pont simulé (alerte et mode
// pilotables par le test). Les bandeaux sont posés par le gabarit du serveur, hors de la page.
afterEach(() => {
  document.body.innerHTML = "";
});

const GOOD = "Correct-Horse-9";

async function boot(path = "/servers/forge/security") {
  const ctx = await mountContext();
  await ctx.router.push(path);
  await ctx.router.isReady();
  const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

const dialog = () => document.querySelector("dialog");
const dialogButton = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("dialog button")].find(
    (b) => b.textContent?.trim() === label,
  );
const toggle = (wrapper: VueWrapper) => wrapper.get("[data-attack-mode-toggle]");

async function confirm(password: string, label = "Activer le mode attaque") {
  typeInto(dialog()?.querySelector("input[type=password]") ?? null, password);
  await flushPromises();
  dialogButton(label)?.click();
  await flushPromises();
}

describe("Sécurité : le mode attaque", () => {
  it("shows the card inactive with the explanation and an available button for an administrator", async () => {
    const { wrapper } = await boot();
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Inactif");
    expect(wrapper.get("[data-attack-mode-panel]").text()).toContain(
      "Le mode attaque permet de ne laisser se connecter que les postes reconnus, le temps d'une attaque.",
    );
    // C34 : plus de « signe » ; éteint, le texte dit ce que le mode FERA (futur), jamais « est actif ».
    expect(wrapper.get("[data-attack-mode-panel]").text()).not.toContain("un seul signe");
    expect(wrapper.get("[data-attack-mode-panel]").text()).not.toContain("est actif");
    expect(wrapper.find("[data-attack-mode-details]").exists()).toBe(false);
    expect(toggle(wrapper).text()).toBe("Activer le mode attaque");
    expect(toggle(wrapper).attributes("aria-disabled")).toBeUndefined();
    expect(wrapper.find("[data-attack-mode-reason]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("asks for the password in the confirmation, then turns the mode on and says so", async () => {
    const { wrapper, bridge } = await boot();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    expect(dialog()?.textContent).toContain("Activer le mode attaque ?");
    expect(dialog()?.textContent).toContain("confirme ton mot de passe");
    // Le bouton attend le mot de passe.
    expect(dialogButton("Activer le mode attaque")?.getAttribute("aria-disabled")).toBe("true");
    await confirm(GOOD);
    expect(dialog()).toBeNull();
    expect(bridge.calls).toContain("attack-mode on");
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Actif");
    expect(toggle(wrapper).text()).toBe("Désactiver le mode attaque");
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Mode attaque activé. Seuls les postes reconnus peuvent se connecter.",
    );
    // Le bandeau permanent est posé par le gabarit, avec le sens écrit.
    expect(wrapper.get("[data-attack-mode-banner]").text()).toContain("Mode attaque actif");
    wrapper.unmount();
  });

  it("keeps the dialog open on a wrong password, with the message under the field, and clears the field", async () => {
    const { wrapper, bridge } = await boot();
    await toggle(wrapper).trigger("click");
    await flushPromises();
    await confirm("Faux-Mot-De-Passe-1");
    expect(dialog()).not.toBeNull();
    expect(dialog()?.textContent).toContain("Mot de passe incorrect.");
    expect(dialog()?.querySelector<HTMLInputElement>("input[type=password]")?.value).toBe("");
    expect(bridge.security.current("forge").attackMode.state).toBe("off");
    // Aucun mot de passe dans le journal des commandes.
    expect(bridge.calls.join("\n")).not.toContain("Faux-Mot-De-Passe-1");
    wrapper.unmount();
  });

  it("disables the button while the request is running and never replays a cut one", async () => {
    const { wrapper, bridge } = await boot();
    bridge.actionMode = "cut";
    await toggle(wrapper).trigger("click");
    await flushPromises();
    await confirm(GOOD);
    // Lien coupé avant la réponse : dit, fermé, jamais rejoué.
    expect(dialog()).toBeNull();
    expect(bridge.calls.filter((call) => call.startsWith("attack-mode"))).toHaveLength(1);
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Le résultat de cette action n'est pas connu.",
    );
    wrapper.unmount();
  });

  it("deactivates with the same confirmation and says so", async () => {
    const { wrapper, bridge } = await boot();
    bridge.security.setMode("forge", "active");
    await flushPromises();
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Actif");
    await toggle(wrapper).trigger("click");
    await flushPromises();
    expect(dialog()?.textContent).toContain("Désactiver le mode attaque ?");
    await confirm(GOOD, "Désactiver le mode attaque");
    expect(dialog()).toBeNull();
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Inactif");
    expect(useToastsStore().items.map((toast) => toast.message)).toContain(
      "Mode attaque désactivé.",
    );
    expect(wrapper.find("[data-attack-mode-banner]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("explains, without touching the server, why a PC with no enrolled key cannot change the mode", async () => {
    const { wrapper, bridge } = await boot();
    bridge.security.setDevice("forge", "none", false);
    await useSecurityStore().load("forge");
    await flushPromises();
    const button = toggle(wrapper);
    expect(button.attributes("aria-disabled")).toBe("true");
    const reason = wrapper.get("[data-attack-mode-reason]").text();
    expect(reason).toContain("Ce poste n'est pas encore enregistré");
    expect(reason).toContain("En attendant, tu peux utiliser la commande sur le serveur.");
    // L'infobulle reprend le texte, et le texte est aussi écrit sous le bouton.
    expect(button.attributes("aria-describedby")).toBeTruthy();
    await button.trigger("click");
    await flushPromises();
    expect(dialog()).toBeNull();
    expect(bridge.calls.some((call) => call.startsWith("attack-mode"))).toBe(false);
    // Sans mot de passe mémorisé : « Se reconnecter » (forge se souvient du sien : absent).
    expect(wrapper.find("[data-security-reconnect]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("says that a read-only account cannot, with the spec's sentence, and offers no reconnection", async () => {
    const { wrapper } = await boot("/servers/salon/security");
    expect(toggle(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-attack-mode-reason]").text()).toBe(
      "Tu n'as pas la permission d'activer le mode attaque. C'est réservé aux administrateurs.",
    );
    expect(wrapper.find("[data-security-reconnect]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("says that an old agent has no attack mode", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.security.setSupported("forge", false);
    await useSecurityStore().load("forge");
    const ctxRouter = wrapper.vm.$router;
    await ctxRouter.push("/servers/forge/security");
    await flushPromises();
    expect(wrapper.get("[data-attack-mode-reason]").text()).toContain(
      "L'agent de ce serveur est trop ancien pour avoir un mode attaque.",
    );
    expect(toggle(wrapper).attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("shows the suspended variant with the minutes and no deactivation lost", async () => {
    const { wrapper, bridge } = await boot();
    bridge.security.setMode("forge", "suspended", { resumesInS: 1200 });
    await flushPromises();
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Suspendu");
    expect(wrapper.get("[data-attack-mode-panel]").text()).toContain("Il reprend dans 20 min.");
    const banner = wrapper.get("[data-attack-mode-banner]").text();
    expect(banner).toContain("Mode attaque suspendu");
    expect(banner).toContain("Le mode attaque reprend dans 20 min.");
    expect(toggle(wrapper).text()).toBe("Désactiver le mode attaque");
    wrapper.unmount();
  });

  it("announces the automatic end and the resumption once", async () => {
    const { wrapper, bridge } = await boot();
    const toasts = useToastsStore();
    bridge.security.setMode("forge", "active");
    await flushPromises();
    bridge.security.setMode("forge", "off", { lastEnd: "auto" });
    await flushPromises();
    expect(toasts.items.map((toast) => toast.message)).toContain(
      "L'attaque semble terminée. Le mode attaque s'est arrêté automatiquement.",
    );
    expect(wrapper.get("[data-attack-mode-state]").text()).toBe("Inactif");
    bridge.security.setMode("forge", "suspended", { resumesInS: 60 });
    bridge.security.setMode("forge", "active");
    await flushPromises();
    expect(toasts.items.map((toast) => toast.message)).toContain(
      "Le mode attaque reprend après 30 minutes de suspension.",
    );
    wrapper.unmount();
  });
});

describe("Alerte d'attaque probable", () => {
  it("shows the banner on every page of the server, with the written title, and none when quiet", async () => {
    const { wrapper, bridge, router } = await boot("/servers/forge/dashboard");
    expect(wrapper.find("[data-security-alert]").exists()).toBe(false);
    bridge.security.setAlert("forge", { own: true });
    await flushPromises();
    const banner = wrapper.get("[data-security-alert]");
    expect(banner.attributes("role")).toBe("alert");
    expect(banner.text()).toContain("Attaque probable détectée");
    expect(banner.text()).toContain(
      "Une attaque probable vise ton identifiant. Clique pour plus d'infos et activer le mode attaque.",
    );
    await router.push("/servers/forge/accounts");
    await flushPromises();
    expect(wrapper.find("[data-security-alert]").exists()).toBe(true);
    bridge.security.setAlert("forge", { own: false });
    await flushPromises();
    expect(wrapper.find("[data-security-alert]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("says how many other accounts are targeted for an administrator, never their names", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.security.setAlert("forge", { own: true, others: 2 });
    await flushPromises();
    expect(wrapper.get("[data-security-alert]").text()).toContain(
      "Une attaque probable vise ton identifiant et 2 autres comptes.",
    );
    bridge.security.setAlert("forge", { own: false, others: 1 });
    await flushPromises();
    expect(wrapper.get("[data-security-alert]").text()).toContain(
      "Une attaque probable vise 1 compte de ce serveur.",
    );
    wrapper.unmount();
  });

  it("activates from the alert with the confirmation, and goes to the page for the details", async () => {
    const { wrapper, bridge, router } = await boot("/servers/forge/dashboard");
    bridge.security.setAlert("forge", { own: true });
    await flushPromises();
    await wrapper.get("[data-security-activate]").trigger("click");
    await flushPromises();
    expect(dialog()?.textContent).toContain("Activer le mode attaque ?");
    await confirm(GOOD);
    expect(bridge.security.current("forge").attackMode.state).toBe("active");
    // Alerte et mode attaque peuvent coexister : deux bandeaux (l activation n est plus proposée).
    expect(wrapper.find("[data-security-alert]").exists()).toBe(true);
    expect(wrapper.find("[data-attack-mode-banner]").exists()).toBe(true);
    await wrapper.get("[data-security-details]").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.name).toBe("security");
    // Sur la page elle-même, « Plus d'infos » n'a plus de sens.
    expect(wrapper.find("[data-security-details]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("lets a read-only account see the alert with its own sentence and a disabled button that says why", async () => {
    const { wrapper, bridge } = await boot("/servers/salon/dashboard");
    bridge.security.setAlert("salon", { own: true });
    await flushPromises();
    const banner = wrapper.get("[data-security-alert]");
    expect(banner.text()).toContain(
      "Tu vois l'alerte, mais seul un administrateur peut activer le mode attaque.",
    );
    const button = wrapper.get("[data-security-activate]");
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(button.attributes("aria-describedby")).toBeTruthy();
    await button.trigger("click");
    await flushPromises();
    expect(dialog()).toBeNull();
    wrapper.unmount();
  });

  it("keeps the banners, in colour, when the link drops, and dates the last known state", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.security.setAlert("forge", { own: true });
    await flushPromises();
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.find("[data-security-alert]").exists()).toBe(true);
    // Le bouton dit que le serveur manque.
    expect(wrapper.get("[data-security-activate]").attributes("aria-disabled")).toBe("true");
    bridge.security.setMode("forge", "active");
    await flushPromises();
    expect(wrapper.get("[data-attack-mode-banner]").text()).toContain("Dernier état connu à");
    // Hors de la surface périmée : ni désaturée ni estompée.
    expect(wrapper.get("[data-attack-mode-banner]").element.closest("[data-stale]")).toBeNull();
    // Le mode est déjà actif : activer de nouveau n'est plus proposé.
    expect(wrapper.find("[data-security-activate]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("Marque de sécurité sur un serveur", () => {
  it("marks the avatar in the rail with the mode first, then suspended, then the alert, and writes it", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    const avatar = () => wrapper.get('[data-server="forge"] [role="img"]');
    expect(avatar().find("[data-mark]").exists()).toBe(false);
    bridge.security.setAlert("forge", { own: true });
    await flushPromises();
    expect(avatar().get("[data-mark]").attributes("data-mark")).toBe("alert");
    expect(avatar().attributes("aria-label")).toContain("alerte de sécurité");
    bridge.security.setMode("forge", "suspended");
    await flushPromises();
    expect(avatar().get("[data-mark]").attributes("data-mark")).toBe("suspended");
    bridge.security.setMode("forge", "active");
    await flushPromises();
    expect(avatar().get("[data-mark]").attributes("data-mark")).toBe("attack");
    expect(avatar().attributes("aria-label")).toContain("mode attaque actif");
    // L'autre serveur n'est pas marqué : le mode attaque est propre à chaque serveur.
    expect(wrapper.get('[data-server="salon"] [role="img"]').find("[data-mark]").exists()).toBe(
      false,
    );
    wrapper.unmount();
  });

  it("writes the mark in the servers list too", async () => {
    const { wrapper, bridge } = await boot("/servers");
    bridge.security.setMode("forge", "active");
    await flushPromises();
    expect(wrapper.get('[data-server-row="forge"] [data-security-tag]').text()).toBe(
      "Mode attaque",
    );
    bridge.security.setMode("forge", "off");
    bridge.security.setAlert("forge", { own: true });
    await flushPromises();
    expect(wrapper.get('[data-server-row="forge"] [data-security-tag]').text()).toBe(
      "Alerte de sécurité",
    );
    wrapper.unmount();
  });
});

describe("Retrait d'un poste", () => {
  it("tells to change the password too when a device is not yours anymore", async () => {
    const { wrapper, bridge } = await boot("/servers/forge/dashboard");
    bridge.devices.seed({ id: "forge" } as never, [
      { name: "salon/0.1.0", current: true },
      { name: "bureau/0.1.0", lastProvedAt: "2020-01-01T00:00:00.000Z" },
    ]);
    await wrapper.vm.$router.push("/servers/forge/security");
    await flushPromises();
    await wrapper.get('button[aria-label="Retirer bureau/0.1.0"]').trigger("click");
    await flushPromises();
    expect(document.querySelector("[data-remove-advice]")?.textContent).toContain(
      "Change aussi ton mot de passe si ce poste n'est plus à toi",
    );
    wrapper.unmount();
  });
});

describe("Lecture de l'état ratée", () => {
  it("says so with a retry instead of a mute disabled button, and the retry reads again", async () => {
    const ctx = await mountContext();
    let broken = true;
    const read = ctx.bridge.getSecurity.bind(ctx.bridge);
    ctx.bridge.getSecurity = async (id: string) => {
      if (broken) throw new LinkCommandError({ kind: "unreachable" });
      return read(id);
    };
    await ctx.router.push("/servers/forge/security");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    expect(toggle(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-attack-mode-reason]").text()).toBe(
      "Impossible de lire l'état de sécurité de ce serveur.",
    );
    broken = false;
    await wrapper.get("[data-security-retry]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-attack-mode-reason]").exists()).toBe(false);
    expect(toggle(wrapper).attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });
  // FIX:01M4DJZAFYE77R5MKKA2NV6CE3 (C34) : actif, la carte dit ce qui se passe en une phrase, le reste est replié.
  it("says what is happening in one sentence when active, the rest folded away", async () => {
    const { wrapper, bridge } = await boot();
    bridge.security.setMode("forge", "active");
    await flushPromises();
    const panel = wrapper.get("[data-attack-mode-panel]");
    expect(panel.text()).toContain("Le mode attaque est actif");
    const details = panel.get("[data-attack-mode-details]");
    expect(details.element.tagName).toBe("DETAILS");
    expect((details.element as HTMLDetailsElement).open).toBe(false);
    expect(details.text()).toContain("Comment ça marche");
    wrapper.unmount();
  });

  // FIX:01M4DJZB43SA08NE46DGEZK213 (C36) : « Plus d'infos » mène à une carte qui dit ce que l'agent sait.
  it("tells the alert on the page: since when, how many other accounts, what to do, one activate button on the whole page", async () => {
    const { wrapper, bridge } = await boot();
    expect(wrapper.find("[data-alert-card]").exists()).toBe(false);
    bridge.security.setAlert("forge", { own: true, others: 2 });
    await flushPromises();
    const card = wrapper.get("[data-alert-card]");
    expect(card.text()).toContain("Ton identifiant est visé");
    expect(card.text()).toContain("2 autres comptes de ce serveur sont visés");
    expect(card.text()).toContain("leurs noms ne sont pas montrés");
    expect(card.get("[data-alert-todo]").text()).toContain("active le mode attaque");
    // Un seul « Activer le mode attaque » sur la page : celui de la carte (ni dans le bandeau, ni dans la carte « Ce qui se passe »).
    const activates = wrapper
      .findAll("button")
      .filter((button) => button.text() === "Activer le mode attaque");
    expect(activates).toHaveLength(1);
    expect(activates[0]?.attributes("data-attack-mode-toggle")).toBeDefined();
    wrapper.unmount();
  });

  it("does not show the alert card to nobody: no alert, no card", async () => {
    const { wrapper } = await boot();
    expect(wrapper.find("[data-alert-card]").exists()).toBe(false);
    wrapper.unmount();
  });
  // HRT-18 (suite) : l'agent dit qu'un effacement est en attente ; la page le dit et dit quoi faire (rien).
  it("tells an administrator that an erasure is pending and that nothing is to be done", async () => {
    const { wrapper, bridge } = await boot();
    expect(wrapper.find("[data-erasure-pending]").exists()).toBe(false);
    bridge.security.setErasurePending("forge", true);
    await wrapper.vm.$nextTick();
    await useSecurityStore().load("forge");
    await flushPromises();
    const note = wrapper.get("[data-erasure-pending]");
    expect(note.text()).toContain("Effacement en attente");
    expect(note.text()).toContain("Rien à faire");
    expect(note.text()).toContain("prochain démarrage de l'agent");
    bridge.security.setErasurePending("forge", false);
    await useSecurityStore().load("forge");
    await flushPromises();
    expect(wrapper.find("[data-erasure-pending]").exists()).toBe(false);
    wrapper.unmount();
  });
  // FIX:01M4DNFDC9KF9FXYJ0H2TC2JKX : l'agent ne le dit qu'aux administrateurs ; un compte en lecture ne le lit jamais.
  it("never tells a read-only account that an erasure is pending", async () => {
    const { wrapper, bridge } = await boot("/servers/salon/security");
    bridge.security.setErasurePending("salon", true);
    await useSecurityStore().load("salon");
    await flushPromises();
    expect(useSecurityStore().of("salon")?.state?.erasurePending).toBe(true);
    expect(wrapper.find("[data-erasure-pending]").exists()).toBe(false);
    wrapper.unmount();
  });
});
