import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import Settings from "./Settings.vue";

describe("Settings page", () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => clearMocks());

  it("shows the General tab, the autostart option and the version", async () => {
    mockIPC((cmd) =>
      cmd === "get_settings"
        ? { launchAtStartup: false, closeHintSeen: false }
        : cmd === "get_app_version"
          ? "0.1.0"
          : undefined,
    );
    const wrapper = mount(Settings);
    await flushPromises();
    expect(wrapper.text()).toContain("Général");
    expect(wrapper.text()).toContain("Lancer Hearth au démarrage de Windows");
    expect(wrapper.text()).toContain("0.1.0");
    const toggle = wrapper.get('[role="switch"]');
    expect(toggle.attributes("aria-checked")).toBe("false");
    expect(toggle.attributes("aria-labelledby")).toBe("launch-at-startup-label");
  });

  it("enables the option through the typed command", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "set_launch_at_startup") {
        seen.push(args);
        return { launchAtStartup: true, closeHintSeen: false };
      }
      return cmd === "get_settings" ? { launchAtStartup: false, closeHintSeen: false } : "0.1.0";
    });
    const wrapper = mount(Settings);
    await flushPromises();
    await wrapper.get('[role="switch"]').trigger("click");
    await flushPromises();
    expect(seen).toEqual([{ enabled: true }]);
    expect(wrapper.get('[role="switch"]').attributes("aria-checked")).toBe("true");
  });

  it("tells the user when settings cannot be read, and locks the switch", async () => {
    mockIPC(() => {
      throw { kind: "store", message: "x" };
    });
    const wrapper = mount(Settings);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe("Impossible de lire tes réglages.");
    expect(wrapper.get('[role="switch"]').attributes("disabled")).toBeDefined();
  });
});
