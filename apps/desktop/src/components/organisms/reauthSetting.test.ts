import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { startedApp } from "@/test/app";
import { confirmDialog, dialogButton, OWN } from "@/test/confirm";
import ReauthSettingCard from "./ReauthSettingCard.vue";

// HRT-30, D2 : la ligne « Demander mon mot de passe » de la page Sécurité.

afterEach(() => {
  document.body.innerHTML = "";
});

async function card() {
  const ctx = await startedApp();
  const wrapper = mount(ReauthSettingCard, {
    props: { serverId: "forge" },
    global: { plugins: [ctx.pinia] },
    attachTo: document.body,
  });
  await flushPromises();
  return { ...ctx, wrapper };
}

const radio = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>('[role="radio"]')].find(
    (button) => button.textContent?.trim() === label,
  );

describe("la ligne « Demander mon mot de passe »", () => {
  it("reads the value of the agent, 5 minutes by default", async () => {
    const { wrapper } = await card();
    expect(wrapper.text()).toContain("Demander mon mot de passe");
    expect(radio("Toutes les 5 minutes")?.getAttribute("aria-checked")).toBe("true");
    expect(radio("À chaque action")?.getAttribute("aria-checked")).toBe("false");
    wrapper.unmount();
  });

  it("changes the value only after a confirmation with the password, then shows the new value", async () => {
    const { wrapper, bridge } = await card();
    radio("À chaque action")?.click();
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain(
      "Ton mot de passe te sera demandé à chaque action d'administration.",
    );
    // Rien n'est parti avant la confirmation.
    expect(bridge.calls.some((call) => call.startsWith("reauth setting"))).toBe(false);
    await confirmDialog("Changer");
    expect(bridge.calls).toContain("reauth setting each");
    expect(bridge.reauth.state("forge").mode).toBe("each");
    expect(radio("À chaque action")?.getAttribute("aria-checked")).toBe("true");
    expect(document.querySelector("dialog")).toBeNull();
    wrapper.unmount();
  });

  it("asks the password even during the delay, and keeps the old value on a wrong one", async () => {
    const { wrapper, bridge } = await card();
    bridge.reauth.open("forge");
    radio("À chaque action")?.click();
    await flushPromises();
    expect(document.querySelector("[data-reauth-field]")).not.toBeNull();
    await confirmDialog("Changer", "Faux-Mot-De-Passe-1");
    expect(document.body.textContent).toContain("Mot de passe actuel incorrect.");
    expect(bridge.reauth.state("forge").mode).toBe("window");
    await confirmDialog("Changer", OWN);
    expect(bridge.reauth.state("forge").mode).toBe("each");
    wrapper.unmount();
  });

  it("is not shown by an agent from before the confirmation of the acts", async () => {
    const ctx = await startedApp();
    ctx.bridge.reauth.setSupported("forge", false);
    const wrapper = mount(ReauthSettingCard, {
      props: { serverId: "forge" },
      global: { plugins: [ctx.pinia] },
      attachTo: document.body,
    });
    await flushPromises();
    expect(wrapper.find("[data-reauth-setting]").exists()).toBe(false);
    expect(dialogButton("Changer")).toBeUndefined();
    wrapper.unmount();
  });
});
