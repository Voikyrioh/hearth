import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import App from "@/App.vue";
import { useSecurityStore } from "@/stores/security";
import { reauthField } from "@/test/confirm";
import { mountContext } from "@/test/mount";
import Settings from "./Settings.vue";

// HRT-26 : le réglage « Alertes de sécurité » (séparé des autres notifications, activé par défaut) et
// la case « Garder ce poste reconnu » de la fenêtre de changement de son mot de passe.
afterEach(() => {
  clearMocks();
  document.body.innerHTML = "";
});

const GOOD = "Correct-Horse-9";
const NEXT = "Another-Long-Pass-91";

function typeInto(el: Element | null, value: string) {
  const input = el as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("Réglage « Alertes de sécurité »", () => {
  it("is its own row, on by default, next to the link notifications, with its help", async () => {
    const ctx = await mountContext();
    mockIPC((cmd) => (cmd === "get_settings" ? { launchAtStartup: false } : "0.1.0"));
    const wrapper = mount(Settings, { global: ctx.global });
    await flushPromises();
    expect(wrapper.text()).toContain("Alertes de sécurité");
    expect(wrapper.text()).toContain(
      "Une notification Windows quand une attaque probable vise ton identifiant, et quand le mode attaque s'arrête tout seul.",
    );
    const switches = wrapper.findAll('[role="switch"]');
    // Démarrage, notifications du lien, alertes de sécurité : trois réglages distincts.
    expect(switches).toHaveLength(3);
    const security = switches[2];
    if (!security) throw new Error("réglage absent");
    const labelId = security.attributes("aria-labelledby") ?? "";
    expect(wrapper.get(`[id="${labelId}"]`).text()).toBe("Alertes de sécurité");
    expect(security.attributes("aria-checked")).toBe("true");
  });

  it("reads and writes through its own typed commands, never the link one", async () => {
    const ctx = await mountContext();
    const calls: Array<{ cmd: string; args: unknown }> = [];
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      if (cmd === "get_settings") return { launchAtStartup: false };
      if (cmd === "get_notify_on_security_alert") return false;
      if (cmd === "set_notify_on_security_alert") return (args as { enabled: boolean }).enabled;
      return cmd === "get_app_version" ? "0.1.0" : true;
    });
    const wrapper = mount(Settings, { global: ctx.global });
    await flushPromises();
    const switchAt = (index: number) => {
      const found = wrapper.findAll('[role="switch"]')[index];
      if (!found) throw new Error("réglage absent");
      return found;
    };
    const security = () => switchAt(2);
    expect(security().attributes("aria-checked")).toBe("false");
    // Le réglage du lien n'a pas bougé : il est lu séparément.
    expect(switchAt(1).attributes("aria-checked")).toBe("true");
    await security().trigger("click");
    await flushPromises();
    expect(calls.filter((call) => call.cmd === "set_notify_on_security_alert")).toEqual([
      { cmd: "set_notify_on_security_alert", args: { enabled: true } },
    ]);
    expect(calls.some((call) => call.cmd === "set_notify_on_link_change")).toBe(false);
    expect(security().attributes("aria-checked")).toBe("true");
  });
});

describe("Case « Garder ce poste reconnu »", () => {
  async function openOwnPassword(server = "forge") {
    const ctx = await mountContext();
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await wrapper.get(`.mine[data-server="${server}"] button`).trigger("click");
    await flushPromises();
    return { ...ctx, wrapper };
  }

  async function submit() {
    typeInto(reauthField(), GOOD);
    typeInto(document.querySelector('input[placeholder="Nouveau mot de passe"]'), NEXT);
    typeInto(document.querySelector('input[placeholder="Confirme le mot de passe"]'), NEXT);
    await flushPromises();
    document.querySelector<HTMLButtonElement>("dialog button[type=submit]")?.click();
    await flushPromises();
  }

  it("is unticked by default, with a line that says what it changes, and sends false", async () => {
    const { wrapper, bridge } = await openOwnPassword();
    const box = document.querySelector<HTMLInputElement>("[data-keep-address] input");
    expect(box?.checked).toBe(false);
    expect(document.querySelector("dialog")?.textContent).toContain("Garder ce poste reconnu");
    expect(document.querySelector("[data-keep-address-help]")?.textContent).toContain(
      "Décoché, il l'oublie avec celles des autres postes.",
    );
    await submit();
    expect(bridge.lastKeepAddress).toBe(false);
    wrapper.unmount();
  });

  it("sends true when ticked, and starts unticked again at the next opening", async () => {
    const { wrapper, bridge } = await openOwnPassword();
    const box = document.querySelector<HTMLInputElement>("[data-keep-address] input");
    box?.click();
    await flushPromises();
    expect(box?.checked).toBe(true);
    await submit();
    expect(bridge.lastKeepAddress).toBe(true);
    await wrapper.get('.mine[data-server="forge"] button').trigger("click");
    await flushPromises();
    expect(document.querySelector<HTMLInputElement>("[data-keep-address] input")?.checked).toBe(
      false,
    );
    wrapper.unmount();
  });

  it("is disabled, with its reason, when the agent is too old to know it", async () => {
    const ctx = await mountContext();
    ctx.bridge.security.setSupported("forge", false);
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await useSecurityStore(ctx.pinia).load("forge");
    await wrapper.get('.mine[data-server="forge"] button').trigger("click");
    await flushPromises();
    expect(document.querySelector<HTMLInputElement>("[data-keep-address] input")?.disabled).toBe(
      true,
    );
    expect(document.querySelector("[data-keep-address-help]")?.textContent).toBe(
      "Disponible quand l'agent de ce serveur est à jour.",
    );
    wrapper.unmount();
  });

  it("says a PC without a key cannot change the password from here, whatever the attack mode", async () => {
    const ctx = await mountContext();
    ctx.bridge.security.setDevice("forge", "none", false);
    ctx.bridge.security.setMode("forge", "active");
    await ctx.router.push("/settings");
    await ctx.router.isReady();
    const wrapper = mount(App, { global: ctx.global, attachTo: document.body });
    await flushPromises();
    await useSecurityStore(ctx.pinia).load("forge");
    await wrapper.get('.mine[data-server="forge"] button').trigger("click");
    await flushPromises();
    // Aucun acte ne part sans la clé de ce poste : la fenêtre l'explique et propose de se reconnecter
    // pour enregistrer ce poste (le mot de passe et la case « Garder ce poste reconnu » n'ont pas lieu).
    const dialog = document.querySelector("dialog");
    expect(dialog?.querySelector("[data-reauth-no-key]")).not.toBeNull();
    expect(dialog?.textContent).toContain("Me reconnecter pour enregistrer ce poste");
    expect(document.querySelector("[data-keep-address]")).toBeNull();
    expect(ctx.bridge.calls.some((c) => c.startsWith("account own-password"))).toBe(false);
    wrapper.unmount();
  });
});
