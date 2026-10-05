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

const ok = { launchAtStartup: true };

describe("settings store", () => {
  beforeEach(() => setActivePinia(createPinia()));
  afterEach(() => clearMocks());

  it("starts with the autostart option off", () => {
    const store = useSettingsStore();
    expect(store.launchAtStartup).toBe(false);
    expect(store.loaded).toBe(false);
    expect(store.version).toBeNull();
  });

  it("loads settings and version from the Rust core", async () => {
    bridge((cmd) =>
      cmd === "get_settings" ? ok : cmd === "get_app_version" ? "0.1.0" : undefined,
    );
    const store = useSettingsStore();
    await store.load();
    expect(store.launchAtStartup).toBe(true);
    expect(store.version).toBe("0.1.0");
    expect(store.loaded).toBe(true);
    expect(store.error).toBeNull();
  });

  it("picks the text from the typed error kind when settings cannot be read", async () => {
    bridge((cmd) => {
      if (cmd === "get_settings") throw { kind: "autostart", message: "registre" };
      return "0.1.0";
    });
    const store = useSettingsStore();
    await store.load();
    expect(store.error).toBe("errors.autostart");
    expect(store.loaded).toBe(false);
    expect(store.version).toBe("0.1.0");
  });

  it("does not blame the settings when only the version is unavailable", async () => {
    bridge((cmd) => {
      if (cmd === "get_app_version") throw new Error("x");
      return ok;
    });
    const store = useSettingsStore();
    await store.load();
    expect(store.error).toBeNull();
    expect(store.loaded).toBe(true);
    expect(store.version).toBeNull();
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
        ? { launchAtStartup: (args as { enabled: boolean }).enabled }
        : undefined,
    );
    const store = useSettingsStore();
    await store.setLaunchAtStartup(true);
    expect(calls).toEqual([{ cmd: "set_launch_at_startup", args: { enabled: true } }]);
    expect(store.launchAtStartup).toBe(true);
    expect(store.error).toBeNull();
    expect(store.saving).toBe(false);
  });

  it("keeps the old value and uses the error kind when the change fails", async () => {
    bridge(() => {
      throw { kind: "autostart", message: "registre" };
    });
    const store = useSettingsStore();
    await store.setLaunchAtStartup(true);
    expect(store.launchAtStartup).toBe(false);
    expect(store.error).toBe("errors.autostart");
    expect(store.saving).toBe(false);
  });

  it("ignores a second change while one is running", async () => {
    let release: () => void = () => {};
    const calls = bridge(
      () => new Promise((resolve) => (release = () => resolve({ launchAtStartup: true }))),
    );
    const store = useSettingsStore();
    const first = store.setLaunchAtStartup(true);
    expect(store.saving).toBe(true);
    await store.setLaunchAtStartup(false);
    expect(calls).toHaveLength(1);
    release();
    await first;
    expect(store.saving).toBe(false);
  });

  it("opens the logs folder and reports a typed failure", async () => {
    const calls = bridge(() => undefined);
    const store = useSettingsStore();
    await store.openLogsFolder();
    expect(calls).toEqual([{ cmd: "open_logs_folder", args: {} }]);
    expect(store.logsError).toBeNull();

    bridge(() => {
      throw { kind: "logs", message: "droits" };
    });
    await store.openLogsFolder();
    expect(store.logsError).toBe("errors.logs");
    expect(store.error).toBeNull();
  });

  it("does not clear a settings read error when the logs folder opens fine", async () => {
    bridge((cmd) => {
      if (cmd === "get_settings") throw { kind: "store", message: "x" };
      return "0.1.0";
    });
    const store = useSettingsStore();
    await store.load();
    expect(store.error).toBe("errors.store");
    await store.openLogsFolder();
    expect(store.error).toBe("errors.store");
    expect(store.logsError).toBeNull();
  });
});
