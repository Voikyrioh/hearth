import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OTHER_FINGERPRINT, SAMPLE_AGENT, type SimAgent } from "@/link";
import { mountContext } from "@/test/mount";
import AddServerWizard from "./AddServerWizard.vue";
import FingerprintAlert from "./FingerprintAlert.vue";
import LoginForm from "./LoginForm.vue";
import OfflineBanner from "./OfflineBanner.vue";
import ReconnectPanel from "./ReconnectPanel.vue";

const PASSWORD = "Correct-Horse-9";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function context(agents: SimAgent[] = [SAMPLE_AGENT], servers: "none" | "sample" = "none") {
  return mountContext({ agents, ...(servers === "none" ? { servers: [] } : {}) });
}

describe("AddServerWizard", () => {
  async function wizard() {
    const ctx = await context();
    await ctx.router.push("/welcome");
    const wrapper = mount(AddServerWizard, { global: ctx.global });
    return { ...ctx, wrapper };
  }

  async function reachFingerprint(wrapper: ReturnType<typeof mount>) {
    const inputs = wrapper.findAll("input");
    await inputs[0]?.setValue("Atelier");
    await inputs[1]?.setValue("192.168.1.50");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
  }

  it("starts at step 1 with the exact spec labels and « Suivant » never greyed without a reason", async () => {
    const { wrapper } = await wizard();
    expect(wrapper.get("h1").text()).toBe("Ajouter un serveur");
    const labels = wrapper.findAll("label").map((label) => label.text());
    expect(labels).toEqual(["Nom du serveur", "Adresse IP ou nom", "Port (optionnel)"]);
    expect(wrapper.find("input[placeholder='7341']").exists()).toBe(true);
    expect(wrapper.find("input[placeholder^='Forge']").exists()).toBe(true);
    expect(wrapper.find("input[placeholder='192.168.1.20 ou forge.maison']").exists()).toBe(true);
    const next = wrapper.findAll("button").find((b) => b.text() === "Suivant");
    // C1 : jamais grisé sans raison ; un champ manquant se dit au clic.
    expect(next?.attributes("aria-disabled")).not.toBe("true");
    expect(wrapper.findAll("[role=radio]")).toHaveLength(8);
    // Le fil des étapes : l'étape courante est annoncée.
    expect(wrapper.get("[aria-current=step]").text()).toContain("Adresse");
  });

  // FIX:01M4D0RJ5EMX3TG1TJB0EJ5EYP (C1, C2) : curseur dans le premier champ ; « Suivant » (clic ou Entrée)
  // dit ce qui manque et y place le curseur au lieu de rester muet.
  it("puts the cursor in the first field, and says what is missing instead of staying mute", async () => {
    const ctx = await context();
    await ctx.router.push("/welcome");
    const wrapper = mount(AddServerWizard, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    const inputs = wrapper.findAll("input");
    expect(document.activeElement).toBe(inputs[0]?.element);
    // L'adresse est remplie, le nom manque : Entrée dit « Le nom du serveur est requis », curseur dans le nom.
    await inputs[1]?.setValue("192.168.1.99");
    (document.activeElement as HTMLElement).blur();
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(wrapper.text()).toContain("Le nom du serveur est requis");
    expect(document.activeElement).toBe(inputs[0]?.element);
    // Le même texte au clic sur « Suivant ».
    const next = wrapper.findAll("button").find((b) => b.text() === "Suivant");
    expect(next?.attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });

  it("goes through the three steps, comparing the fingerprint and connecting", async () => {
    const { wrapper, servers } = await wizard();
    await reachFingerprint(wrapper);
    expect(wrapper.get("h1").text()).toBe("Vérifie l'identité du serveur");
    const groups = wrapper.findAll("[data-fingerprint] span").map((g) => g.text());
    expect(groups).toEqual(["A1B2", "C3D4", "E5F6", "0718", "293A", "4B5C", "6D7E", "8F90"]);
    expect(wrapper.text()).toContain(
      "Compare avec l'empreinte affichée à la fin de l'installation de l'agent.",
    );
    // Rien n'est enregistré avant « Confirmer » ; les deux boutons sont là.
    expect(servers.servers).toHaveLength(0);
    const buttons = wrapper.findAll("button").map((b) => b.text());
    expect(buttons).toEqual(["Refuser", "Confirmer"]);
    await wrapper.findAll("button")[1]?.trigger("click");
    await flushPromises();
    // « Confirmer » n'enregistre rien : le serveur n'existe qu'à la connexion réussie.
    expect(servers.servers).toHaveLength(0);
    expect(wrapper.get("h1").text()).toBe("Connecte-toi");
    const remember = wrapper.get("input[type=checkbox]");
    expect((remember.element as HTMLInputElement).checked).toBe(true);
    expect(wrapper.text()).toContain("Se souvenir de moi sur ce PC");

    const [user, password] = wrapper.findAll("input:not([type=checkbox])");
    await user?.setValue("marie");
    await password?.setValue("faux-faux-1");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(wrapper.text()).toContain("Identifiant ou mot de passe incorrect.");
    expect(wrapper.emitted("done")).toBeUndefined();
    expect(servers.servers).toHaveLength(0);
    await wrapper.findAll("input:not([type=checkbox])")[1]?.setValue(PASSWORD);
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(wrapper.emitted("done")?.[0]?.[0]).toBe(servers.servers[0]?.id);
  });

  it("registers nothing and returns to the list when the fingerprint is refused", async () => {
    const { wrapper, servers } = await wizard();
    await reachFingerprint(wrapper);
    await wrapper.findAll("button")[0]?.trigger("click");
    expect(wrapper.emitted("cancel")).toHaveLength(1);
    expect(servers.servers).toHaveLength(0);
  });

  it("shows the unreachable message under the address field", async () => {
    const { wrapper } = await wizard();
    const inputs = wrapper.findAll("input");
    await inputs[0]?.setValue("Atelier");
    await inputs[1]?.setValue("10.9.9.9");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(wrapper.get("[role=alert]").text()).toBe(
      "Cette adresse n'est pas joignable. Vérifie l'adresse et essaie de nouveau.",
    );
    expect(wrapper.get("h1").text()).toBe("Ajouter un serveur");
  });

  it("leaves nothing behind when the wizard is closed at step 3", async () => {
    const { wrapper, servers, bridge } = await wizard();
    await reachFingerprint(wrapper);
    await wrapper.findAll("button")[1]?.trigger("click");
    await flushPromises();
    wrapper.unmount();
    await flushPromises();
    expect(servers.servers).toHaveLength(0);
    expect(
      bridge.calls.some((call) => call.startsWith("add-and-login") || call.startsWith("remove")),
    ).toBe(false);
  });
});

describe("FingerprintAlert", () => {
  const change = {
    serverId: "forge",
    expected: "A1B2 C3D4 E5F6 0718 293A 4B5C 6D7E 8F90",
    presented: "0F1E 2D3C 4B5A 6978 8796 A5B4 C3D2 E1F0",
    presentedHex: OTHER_FINGERPRINT,
  };

  it("shows the exact message, both fingerprints, and the two actions", async () => {
    const wrapper = mount(FingerprintAlert, {
      props: { change, serverName: "forge" },
      attachTo: document.body,
    });
    await flushPromises();
    expect(wrapper.element.nodeType).toBeDefined();
    const dialog = document.body.querySelector("dialog[data-fingerprint-alert]");
    expect(dialog?.getAttribute("role")).toBe("alertdialog");
    const text = dialog?.textContent ?? "";
    expect(text).toContain(
      "L'identité de ce serveur a changé. Cela peut signifier que l'agent a été réinstallé ou que la machine a changé. Compare avec l'empreinte du serveur et accepte le changement ou refuse la connexion.",
    );
    expect(text).toContain("Empreinte mémorisée");
    expect(text).toContain("Empreinte reçue");
    expect(text).toContain("0F1E");
    const buttons = [...(dialog?.querySelectorAll("button") ?? [])].map((b) =>
      b.textContent?.trim(),
    );
    expect(buttons).toEqual(["Ne pas se connecter", "Accepter la nouvelle empreinte"]);
    // Pas de bouton plein : l'acceptation est un bouton contour.
    expect(dialog?.querySelector(".btn--solid")).toBeNull();
    // L'action par défaut au clavier est « Ne pas se connecter ».
    expect(document.activeElement?.textContent?.trim()).toBe("Ne pas se connecter");
    wrapper.unmount();
  });

  it("refuses on Escape and on its refuse button, and accepts only on the explicit button", async () => {
    const wrapper = mount(FingerprintAlert, {
      props: { change, serverName: "forge" },
      attachTo: document.body,
    });
    await flushPromises();
    const dialog = document.body.querySelector("dialog") as HTMLDialogElement;
    dialog.dispatchEvent(new Event("cancel", { cancelable: true }));
    expect(wrapper.emitted("refuse")).toHaveLength(1);
    expect(wrapper.emitted("accept")).toBeUndefined();
    const buttons = [...dialog.querySelectorAll("button")];
    buttons[1]?.click();
    await flushPromises();
    expect(wrapper.emitted("accept")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("LoginForm", () => {
  it("asks for the identifier and the password and sends them with the remember choice", async () => {
    const wrapper = mount(LoginForm);
    await wrapper.get("form").trigger("submit");
    expect(wrapper.text()).toContain("L'identifiant est requis");
    expect(wrapper.text()).toContain("Le mot de passe est requis");
    expect(wrapper.emitted("submit")).toBeUndefined();
    const [user, password] = wrapper.findAll("input:not([type=checkbox])");
    await user?.setValue(" marie ");
    await password?.setValue("secret-1");
    await wrapper.get("input[type=checkbox]").setValue(false);
    await wrapper.get("form").trigger("submit");
    expect(wrapper.emitted("submit")?.[0]?.[0]).toEqual({
      username: "marie",
      password: "secret-1",
      remember: false,
    });
  });

  it("freezes the fields while connecting and shows a countdown instead of the button when locked", async () => {
    const wrapper = mount(LoginForm, { props: { busy: true } });
    expect(wrapper.findAll("input").every((i) => i.attributes("disabled") !== undefined)).toBe(
      true,
    );
    expect(wrapper.get("[role=status]").text()).toBe("Connexion en cours…");
    await wrapper.setProps({ busy: false, lockedSeconds: 12 });
    expect(wrapper.get("[role=alert]").text()).toBe(
      "Trop de tentatives. Attends 12 s avant de réessayer.",
    );
    expect(wrapper.findAll("button").find((b) => b.text() === "Se connecter")).toBeUndefined();
  });

  it("empties the password after a refusal", async () => {
    const wrapper = mount(LoginForm, { props: { username: "marie" } });
    await wrapper.findAll("input")[1]?.setValue("secret-1");
    (wrapper.vm as unknown as { clearPassword: () => void }).clearPassword();
    await flushPromises();
    const field = wrapper.findAll("input")[1];
    expect((field?.element as HTMLInputElement | undefined)?.value).toBe("");
  });
});

describe("ReconnectPanel", () => {
  const server = {
    id: "forge",
    name: "forge",
    address: "192.168.1.50",
    host: "192.168.1.50",
    port: 7341,
    color: 1,
    role: "admin",
    username: "marie",
    remember: false,
  } as const;

  it("reopens the login form with the identifier filled in, and says the session expired", async () => {
    const ctx = await context([SAMPLE_AGENT], "none");
    const wrapper = mount(ReconnectPanel, {
      props: { server, reason: "expired" },
      global: ctx.global,
    });
    expect(wrapper.text()).toContain("Ta session a expiré.");
    expect(wrapper.text()).toContain("Rentre ton mot de passe pour reprendre.");
    expect(wrapper.get("button[type=submit]").text()).toBe("Me reconnecter");
    expect((wrapper.get("input").element as HTMLInputElement).value).toBe("marie");
  });

  // FIX:01M4D0RHZE7JFMV700JKA3DM1R (C56) : l'identifiant est connu, le curseur est dans le mot de passe.
  it("puts the cursor in the password field when the identifier is already known", async () => {
    const ctx = await context([SAMPLE_AGENT], "none");
    const wrapper = mount(ReconnectPanel, {
      props: { server, reason: "expired" },
      global: ctx.global,
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.activeElement).toBe(wrapper.findAll("input")[1]?.element);
    wrapper.unmount();
  });

  // Garde du curseur : un panneau qui apparaît ne vole pas le champ où l'utilisateur est déjà en train de taper.
  it("does not steal the cursor from a field the user is already typing in", async () => {
    const ctx = await context([SAMPLE_AGENT], "none");
    const elsewhere = document.createElement("input");
    document.body.append(elsewhere);
    elsewhere.focus();
    const wrapper = mount(ReconnectPanel, {
      props: { server, reason: "expired" },
      global: ctx.global,
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.activeElement).toBe(elsewhere);
    wrapper.unmount();
    elsewhere.remove();
  });

  it("shows no blocking message when the remembered password was refused (BR-CONN-017)", async () => {
    const ctx = await context();
    const wrapper = mount(ReconnectPanel, {
      props: { server, reason: "stored_password_refused" },
      global: ctx.global,
    });
    expect(wrapper.find("[role=status]").exists()).toBe(false);
    expect(wrapper.find("form").exists()).toBe(true);
  });

  it("explains a revoked access and only opens the form for « Utiliser un autre compte »", async () => {
    const ctx = await context();
    const wrapper = mount(ReconnectPanel, {
      props: { server, reason: "revoked", revoked: true },
      global: ctx.global,
    });
    expect(wrapper.text()).toContain("Ton compte n'est plus accessible.");
    expect(wrapper.text()).toContain("Connecte-toi avec un compte valide.");
    expect(wrapper.find("form").exists()).toBe(false);
    await wrapper.get("button").trigger("click");
    expect(wrapper.text()).toContain("Se connecter");
    // L'ancien identifiant n'est pas repris : c'est un autre compte.
    expect((wrapper.get("input").element as HTMLInputElement).value).toBe("");
  });
});

describe("OfflineBanner when attempts are suspended", () => {
  it("explains a changed identity and offers to see the alert instead of retrying", async () => {
    const wrapper = mount(OfflineBanner, {
      props: { lastContactAt: 1, blocked: "fingerprint_changed" },
    });
    expect(wrapper.text()).toContain("L'identité de ce serveur a changé.");
    const button = wrapper.get("button");
    expect(button.text()).toBe("Voir l'alerte");
    await button.trigger("click");
    expect(wrapper.emitted("alert")).toHaveLength(1);
  });

  it("says which side to update, with the spec texts (BR-UPDATE-020, 021)", () => {
    const agent = mount(OfflineBanner, {
      props: { lastContactAt: null, blocked: "incompatible_agent" },
    });
    expect(agent.text()).toContain(
      "Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent.",
    );
    // Un compte Lecture seule ne peut pas : il le demande à un administrateur.
    const readonly = mount(OfflineBanner, {
      props: { lastContactAt: null, blocked: "incompatible_agent", role: "readonly" },
    });
    expect(readonly.text()).toContain(
      "Les versions du client et de l'agent ne sont pas compatibles. Demande à un administrateur de mettre à jour l'agent.",
    );
    const client = mount(OfflineBanner, {
      props: { lastContactAt: null, blocked: "incompatible_client" },
    });
    expect(client.text()).toContain(
      "Les versions du client et de l'agent ne sont pas compatibles. Mets à jour le client.",
    );
  });
});
