import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import Settings from "./Settings.vue";

describe("Settings page", () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => clearMocks());

  it("shows the General section, the autostart option, the logs button and the version", async () => {
    mockIPC((cmd) =>
      cmd === "get_settings"
        ? { launchAtStartup: false }
        : cmd === "get_app_version"
          ? "0.1.0"
          : undefined,
    );
    const wrapper = mount(Settings);
    await flushPromises();
    expect(wrapper.get("h2").text()).toBe("Général");
    expect(wrapper.text()).toContain("Lancer Hearth au démarrage de Windows");
    expect(wrapper.text()).toContain("Ouvrir le dossier des journaux");
    expect(wrapper.text()).toContain("0.1.0");
    const toggle = wrapper.get('[role="switch"]');
    expect(toggle.attributes("aria-checked")).toBe("false");
    const labelId = toggle.attributes("aria-labelledby") ?? "";
    expect(wrapper.get(`[id="${labelId}"]`).text()).toBe("Lancer Hearth au démarrage de Windows");
  });

  it("enables the option through the typed command", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "set_launch_at_startup") {
        seen.push(args);
        return { launchAtStartup: true };
      }
      return cmd === "get_settings" ? { launchAtStartup: false } : "0.1.0";
    });
    const wrapper = mount(Settings);
    await flushPromises();
    await wrapper.get('[role="switch"]').trigger("click");
    await flushPromises();
    expect(seen).toEqual([{ enabled: true }]);
    expect(wrapper.get('[role="switch"]').attributes("aria-checked")).toBe("true");
  });

  it("opens the logs folder from the button", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      return cmd === "get_settings" ? { launchAtStartup: false } : "0.1.0";
    });
    const wrapper = mount(Settings);
    await flushPromises();
    await wrapper.get("button.btn--secondary").trigger("click");
    expect(calls).toContain("open_logs_folder");
  });

  it("explains why the switch is inert when settings cannot be read", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_settings") throw { kind: "store", message: "x" };
      return "0.1.0";
    });
    const wrapper = mount(Settings);
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe(
      "Impossible de lire ou d'enregistrer tes réglages.",
    );
    const toggle = wrapper.get('[role="switch"]');
    expect(toggle.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Disponible dès que tes réglages sont lus.",
    );
    expect(wrapper.text()).toContain("0.1.0");
  });

  it("says the version is unavailable without blaming the settings", async () => {
    mockIPC((cmd) => {
      if (cmd === "get_app_version") throw new Error("x");
      return { launchAtStartup: false };
    });
    const wrapper = mount(Settings);
    await flushPromises();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("indisponible");
    expect(wrapper.get('[role="switch"]').attributes("aria-disabled")).toBeUndefined();
  });

  it("offers the link notifications option, on by default, and changes it through the typed command", async () => {
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "get_notify_on_link_change") return true;
      if (cmd === "set_notify_on_link_change") {
        seen.push(args);
        return (args as { enabled: boolean }).enabled;
      }
      return cmd === "get_settings" ? { launchAtStartup: false } : "0.1.0";
    });
    const wrapper = mount(Settings);
    await flushPromises();
    const headings = wrapper.findAll("h2").map((h) => h.text());
    expect(headings).toEqual(["Général", "Notifications"]);
    const toggle = wrapper.findAll('[role="switch"]')[1];
    const labelId = toggle?.attributes("aria-labelledby") ?? "";
    expect(wrapper.get(`[id="${labelId}"]`).text()).toBe(
      "Notifier quand un serveur devient hors ligne ou revient",
    );
    expect(toggle?.attributes("aria-checked")).toBe("true");
    await toggle?.trigger("click");
    await flushPromises();
    expect(seen).toEqual([{ enabled: false }]);
    expect(wrapper.findAll('[role="switch"]')[1]?.attributes("aria-checked")).toBe("false");
  });
});
