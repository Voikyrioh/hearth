import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import { h } from "vue";
import type { ActReport } from "@/composables/useReauth";
import { fr } from "@/i18n/fr";
import type { AdminActKind } from "@/link";
import { startedApp } from "@/test/app";
import { confirmDialog, dialogButton, OWN, reauthField, typeInto } from "@/test/confirm";
import AdminActDialog from "./AdminActDialog.vue";

// HRT-30 : LA fenêtre de confirmation des actes d'administration, contre le pont simulé qui applique les
// règles de l'agent (mot de passe, élévation de 5 minutes, agent ancien, poste sans clé). Le vrai agent et
// la preuve de la clé sont prouvés côté Rust (`crates/hearth-link/tests/admin_reauth.rs`).

afterEach(() => {
  document.body.innerHTML = "";
  vi.useRealTimers();
});

const PAUL = "SIMACCOUNT0002";

/** La fenêtre pour fermer les sessions de paul, avec un champ d'acte (pour vérifier qu'il est gardé). */
async function open(kind: AdminActKind = "sessions_revoke") {
  const ctx = await startedApp();
  const sent: Array<string | null> = [];
  const perform = async (adminPassword: string | null): Promise<ActReport> => {
    sent.push(adminPassword);
    const outcome = await ctx.bridge.closeAccountSessions("forge", PAUL, adminPassword);
    return outcome.kind === "done" ? { kind: "done" } : (outcome as ActReport);
  };
  const wrapper = mount(AdminActDialog, {
    props: {
      open: true,
      serverId: "forge",
      kind,
      title: "Fermer ?",
      submitLabel: "Fermer",
      perform,
    },
    slots: { default: () => h("input", { placeholder: "saisie de l'acte" }) },
    global: { plugins: [ctx.pinia] },
    attachTo: document.body,
  });
  await flushPromises();
  return { ...ctx, wrapper, sent };
}

const text = () => document.body.textContent ?? "";

describe("AdminActDialog", () => {
  it("asks « Ton mot de passe » when the agent announces the confirmation, and sends nothing without it", async () => {
    const { wrapper, sent } = await open();
    expect(reauthField()).not.toBeNull();
    expect(text()).toContain("Ton mot de passe");
    expect(dialogButton("Fermer")?.getAttribute("aria-disabled")).toBe("true");
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(sent).toEqual([]);
    wrapper.unmount();
  });

  it("puts the cursor in the password field when the act has no field of its own", async () => {
    const ctx = await startedApp();
    const wrapper = mount(AdminActDialog, {
      props: {
        open: true,
        serverId: "forge",
        kind: "attack_mode_enable",
        title: "Activer",
        submitLabel: "Activer",
        perform: async () => ({ kind: "done" }) as ActReport,
      },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.activeElement).toBe(reauthField());
    wrapper.unmount();
  });

  it("empties the field after every send and at every opening, whatever the answer", async () => {
    const { wrapper } = await open();
    typeInto(reauthField(), "Faux-Mot-De-Passe-1");
    await flushPromises();
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(reauthField()?.value).toBe("");
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(reauthField()?.value).toBe("");
    wrapper.unmount();
  });

  it("shows a wrong password under the field, keeps the window open, and later the wait of the agent", async () => {
    const { wrapper } = await open();
    typeInto(reauthField(), "Faux-Mot-De-Passe-1");
    await flushPromises();
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(text()).toContain("Mot de passe actuel incorrect.");
    expect(wrapper.emitted("close")).toBeUndefined();
    for (let attempt = 0; attempt < 5; attempt++) {
      typeInto(reauthField(), "Faux-Mot-De-Passe-1");
      await flushPromises();
      dialogButton("Fermer")?.click();
      await flushPromises();
    }
    expect(text()).toContain("Trop d'essais. Attends 30 s avant de réessayer.");
    expect(wrapper.emitted("close")).toBeUndefined();
    wrapper.unmount();
  });

  it("closes on success, and the password is sent once, to the act and nowhere else", async () => {
    const { wrapper, sent, bridge, pinia } = await open();
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(sent).toEqual([OWN]);
    expect(wrapper.emitted("close")).toHaveLength(1);
    expect(JSON.stringify(bridge.calls)).not.toContain(OWN);
    expect(JSON.stringify(pinia.state.value)).not.toContain(OWN);
    wrapper.unmount();
  });

  it("during the 5 minutes a covered act has no field and shows the time left; an uncovered one still asks", async () => {
    const first = await open();
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Fermer")?.click();
    await flushPromises();
    first.wrapper.unmount();
    document.body.innerHTML = "";
    // Même pont : fermer les sessions est couvert, l'élévation est ouverte.
    const wrapper = mount(AdminActDialog, {
      props: {
        open: true,
        serverId: "forge",
        kind: "sessions_revoke",
        title: "Fermer ?",
        submitLabel: "Fermer",
        perform: async () => ({ kind: "done" }) as ActReport,
      },
      global: { plugins: [first.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(reauthField()).toBeNull();
    expect(document.querySelector("[data-reauth-elevated]")?.textContent).toMatch(/\d+ min \d+ s/);
    expect(dialogButton("Fermer")?.getAttribute("aria-disabled")).toBeNull();
    await wrapper.setProps({ kind: "account_password" });
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    wrapper.unmount();
  });

  it("asks again without losing the entry of the act when the delay closed on the agent side", async () => {
    const { wrapper, bridge, sent } = await open();
    typeInto(reauthField(), OWN);
    await flushPromises();
    dialogButton("Fermer")?.click();
    await flushPromises();
    // Une autre fenêtre : le délai est ouvert, le champ absent.
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(reauthField()).toBeNull();
    typeInto(document.querySelector('input[placeholder="saisie de l\'acte"]'), "ce que j'ai tapé");
    // Le délai se ferme chez l'agent PENDANT que la fenêtre est ouverte (la fenêtre ne le sait pas).
    bridge.reauth.close("forge");
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(sent.at(-1)).toBeNull();
    expect(wrapper.emitted("close")?.length ?? 0).toBe(1);
    expect(text()).toContain("Le délai est terminé.");
    expect(reauthField()).not.toBeNull();
    expect(
      document.querySelector<HTMLInputElement>('input[placeholder="saisie de l\'acte"]')?.value,
    ).toBe("ce que j'ai tapé");
    // Et l'acte repart avec le mot de passe, un envoi neuf.
    await confirmDialog("Fermer");
    expect(sent.at(-1)).toBe(OWN);
    wrapper.unmount();
  });

  it("says to update an agent that does not announce the confirmation, and sends nothing", async () => {
    const ctx = await startedApp();
    ctx.bridge.reauth.setSupported("forge", false);
    const perform = vi.fn(async (): Promise<ActReport> => ({ kind: "done" }));
    const wrapper = mount(AdminActDialog, {
      props: {
        open: true,
        serverId: "forge",
        kind: "sessions_revoke",
        title: "Fermer ?",
        submitLabel: "Fermer",
        perform,
      },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(reauthField()).toBeNull();
    expect(document.querySelector("[data-reauth-elevated]")).toBeNull();
    expect(document.querySelector("[data-reauth-agent-old]")?.textContent).toContain(
      "Mets à jour l'agent",
    );
    dialogButton("Fermer")?.click();
    await flushPromises();
    expect(perform).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("explains a PC without a key, offers to reconnect, and lets nothing leave", async () => {
    const { wrapper, bridge, sent } = await open();
    bridge.security.setDevice("forge", "none", false);
    await wrapper.setProps({ open: false });
    await wrapper.setProps({ open: true });
    await flushPromises();
    expect(document.querySelector("[data-reauth-no-key]")).not.toBeNull();
    expect(text()).toContain("Ce poste n'est pas encore enregistré");
    expect(text()).toContain("Me reconnecter pour enregistrer ce poste");
    expect(reauthField()).toBeNull();
    expect(
      document.querySelector("dialog button[type=submit]")?.getAttribute("aria-disabled"),
    ).toBe("true");
    expect(sent).toEqual([]);
    expect(bridge.calls.some((call) => call.startsWith("account sessions"))).toBe(false);
    const logout = vi.spyOn(bridge, "logout");
    dialogButton("Me reconnecter pour enregistrer ce poste")?.click();
    await flushPromises();
    expect(logout).toHaveBeenCalledWith("forge");
    wrapper.unmount();
  });

  it("says it cannot read what the server expects, and offers to retry", async () => {
    const ctx = await startedApp();
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    const wrapper = mount(AdminActDialog, {
      props: {
        open: true,
        serverId: "forge",
        kind: "sessions_revoke",
        title: "Fermer ?",
        submitLabel: "Fermer",
        perform: async () => ({ kind: "done" }) as ActReport,
      },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.querySelector("[data-reauth-failed]")).not.toBeNull();
    expect(text()).toContain("Impossible de lire ce que le serveur attend de toi.");
    expect(
      document.querySelector("dialog button[type=submit]")?.getAttribute("aria-disabled"),
    ).toBe("true");
    ctx.bridge.setState("forge", "connected");
    await flushPromises();
    dialogButton("Réessayer")?.click();
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    wrapper.unmount();
  });

  it("writes the texts of the confirmation in the second person, without a long dash", () => {
    const texts = JSON.stringify(fr.reauth);
    expect(texts).not.toContain("—");
    for (const word of [" votre ", " vos ", " vous "])
      expect(texts.toLowerCase()).not.toContain(word);
  });
});
