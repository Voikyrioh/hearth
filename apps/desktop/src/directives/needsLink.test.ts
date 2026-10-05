import { type DOMWrapper, flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { mountContext } from "@/test/mount";
import { vNeedsLink } from "./needsLink";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

async function setup(serverId = "forge") {
  const ctx = await mountContext();
  ctx.servers.setCurrent(serverId);
  const clicks = vi.fn();
  const Host = defineComponent({
    components: { HButton },
    directives: { "needs-link": vNeedsLink },
    props: { admin: { type: Boolean, default: false } },
    setup: () => ({ clicks }),
    template: `
      <div>
        <HButton v-needs-link @click="clicks('component')">Agir</HButton>
        <button type="button" v-needs-link="{ role: 'admin' }" @click="clicks('admin')">Admin</button>
      </div>`,
  });
  const wrapper = mount(Host, { global: ctx.global, attachTo: document.body });
  const action = wrapper.findAll("button")[0] as DOMWrapper<HTMLButtonElement>;
  const admin = wrapper.findAll("button")[1] as DOMWrapper<HTMLButtonElement>;
  return { ...ctx, wrapper, clicks, action, admin };
}

describe("v-needs-link", () => {
  it("leaves the control active and silent while the link is connected", async () => {
    const { action, clicks, wrapper } = await setup();
    expect(action.attributes("aria-disabled")).toBeUndefined();
    expect(action.attributes("title")).toBeUndefined();
    await action.trigger("click");
    expect(clicks).toHaveBeenCalledWith("component");
    wrapper.unmount();
  });

  it.each([
    ["reconnecting", "Indisponible pendant la reconnexion au serveur."],
    ["offline", "Indisponible tant que le serveur est hors ligne."],
    ["session_expired", "Indisponible : ta session a expiré."],
    ["access_revoked", "Indisponible : ton compte n'est plus accessible."],
  ] as const)("disables with the exact explanation when the link is %s", async (state, text) => {
    const { bridge, action, wrapper } = await setup();
    bridge.setState("forge", state);
    await flushPromises();
    expect(action.attributes("aria-disabled")).toBe("true");
    expect(action.attributes("disabled")).toBeUndefined();
    expect(action.attributes("title")).toBe(text);
    wrapper.unmount();
  });

  it("blocks the click before the component's own handler, and gives it back on return", async () => {
    const { bridge, action, clicks, wrapper } = await setup();
    bridge.setState("forge", "offline");
    await flushPromises();
    await action.trigger("click");
    expect(clicks).not.toHaveBeenCalled();
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(action.attributes("aria-disabled")).toBeUndefined();
    expect(action.attributes("title")).toBeUndefined();
    await action.trigger("click");
    expect(clicks).toHaveBeenCalledTimes(1);
    wrapper.unmount();
  });

  it("keeps the keyboard focus on a blocked control (aria-disabled, not disabled)", async () => {
    const { bridge, action, wrapper } = await setup();
    bridge.setState("forge", "offline");
    await flushPromises();
    (action.element as HTMLElement).focus();
    expect(document.activeElement).toBe(action.element);
    wrapper.unmount();
  });

  it("only follows the CURRENT server: another server going offline changes nothing", async () => {
    const { bridge, action, wrapper } = await setup("forge");
    bridge.setState("salon", "offline");
    await flushPromises();
    expect(action.attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("requires the administrator role when asked, even with a connected link", async () => {
    const { admin, wrapper, clicks } = await setup("salon");
    expect(admin.attributes("aria-disabled")).toBe("true");
    expect(admin.attributes("title")).toBe("Réservé aux administrateurs.");
    await admin.trigger("click");
    expect(clicks).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it("lets an administrator through on a connected link", async () => {
    const { admin, wrapper } = await setup("forge");
    expect(admin.attributes("aria-disabled")).toBeUndefined();
    wrapper.unmount();
  });

  it("explains itself when there is no current server", async () => {
    const { servers, action, wrapper } = await setup();
    servers.setCurrent(null);
    await flushPromises();
    expect(action.attributes("title")).toBe("Indisponible : aucun serveur sélectionné.");
    wrapper.unmount();
  });

  it("restores a control's own title and stops watching when removed", async () => {
    const ctx = await mountContext();
    ctx.servers.setCurrent("forge");
    const shown = ref(true);
    const Host = defineComponent({
      directives: { "needs-link": vNeedsLink },
      setup: () => ({ shown }),
      template: '<button v-if="shown" v-needs-link title="Mon infobulle">x</button>',
    });
    const wrapper = mount(Host, { global: ctx.global });
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.get("button").attributes("title")).toContain("hors ligne");
    ctx.bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.get("button").attributes("title")).toBe("Mon infobulle");
    shown.value = false;
    await flushPromises();
    expect(wrapper.find("button").exists()).toBe(false);
  });
});
