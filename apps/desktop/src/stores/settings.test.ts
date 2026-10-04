import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useSettingsStore } from "./settings";

type Call = { cmd: string; args?: unknown };

function bridge(handler: (cmd: string, args?: unknown) => unknown) {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    return handler(cmd, args);
  });
  return calls;
}

describe("settings store", () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => clearMocks());

  it("starts with the autostart option off", () => {
    const store = useSettingsStore();
    expect(store.launchAtStartup).toBe(false);
    expect(store.loaded).toBe(false);
  });

  it("loads settings and version from the Rust core", async () => {
    bridge((cmd) =>
      cmd === "get_settings"
        ? { launchAtStartup: true, closeHintSeen: false }
        : cmd === "get_app_version"
          ? "0.1.0"
          : undefined,
    );
    const store = useSettingsStore();
    await store.load();
    expect(store.launchAtStartup).toBe(true);
    expect(store.version).toBe("0.1.0");
    expect(store.loaded).toBe(true);
    expect(store.error).toBeNull();
  });

  it("reports a typed Rust error when loading fails", async () => {
    bridge(() => {
      throw { kind: "store", message: "disque" };
    });
    const store = useSettingsStore();
    await store.load();
    expect(store.error).toBe("settings.loadError");
    expect(store.loaded).toBe(false);
  });

  it("reports an error when there is no bridge at all", async () => {
    clearMocks();
    const store = useSettingsStore();
    await store.load();
    expect(store.error).toBe("settings.loadError");
  });

  it("sends the new value and follows what Rust reports (BR-CLIENT-007)", async () => {
    const calls = bridge((cmd, args) =>
      cmd === "set_launch_at_startup"
        ? { launchAtStartup: (args as { enabled: boolean }).enabled, closeHintSeen: false }
        : undefined,
    );
    const store = useSettingsStore();
    await store.setLaunchAtStartup(true);
    expect(calls).toEqual([{ cmd: "set_launch_at_startup", args: { enabled: true } }]);
    expect(store.launchAtStartup).toBe(true);
    expect(store.error).toBeNull();
  });

  it("keeps the old value and shows an error when the change fails", async () => {
    bridge(() => {
      throw { kind: "autostart", message: "registre" };
    });
    const store = useSettingsStore();
    await store.setLaunchAtStartup(true);
    expect(store.launchAtStartup).toBe(false);
    expect(store.error).toBe("settings.saveError");
  });
});
