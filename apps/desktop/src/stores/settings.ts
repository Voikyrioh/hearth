import { defineStore } from "pinia";
import { ref } from "vue";
import { commands } from "@/bindings";
import { errorKey, type MessageKey } from "@/i18n";

/**
 * Réglages locaux, lus et écrits uniquement par les commandes typées du cœur Rust.
 * Chaque source échoue séparément : réglages illisibles, version indisponible et
 * dossier des journaux inaccessible ne se masquent pas entre eux.
 */
export const useSettingsStore = defineStore("settings", () => {
  const launchAtStartup = ref(false);
  const loaded = ref(false);
  const saving = ref(false);
  const version = ref<string | null>(null);
  const error = ref<MessageKey | null>(null);

  async function load() {
    try {
      const result = await commands.getSettings();
      if (result.status === "ok") {
        launchAtStartup.value = result.data.launchAtStartup;
        loaded.value = true;
        error.value = null;
      } else {
        error.value = errorKey(result.error.kind);
      }
    } catch {
      // Hors de l'application (navigateur de revue sans pont) ou pont en panne.
      error.value = "settings.loadError";
    }
    try {
      version.value = await commands.getAppVersion();
    } catch {
      version.value = null;
    }
  }

  async function setLaunchAtStartup(enabled: boolean) {
    if (saving.value) return;
    saving.value = true;
    try {
      const result = await commands.setLaunchAtStartup(enabled);
      if (result.status === "ok") {
        launchAtStartup.value = result.data.launchAtStartup;
        error.value = null;
      } else {
        error.value = errorKey(result.error.kind);
      }
    } catch {
      error.value = "settings.saveError";
    } finally {
      saving.value = false;
    }
  }

  async function openLogsFolder() {
    try {
      const result = await commands.openLogsFolder();
      error.value = result.status === "ok" ? null : errorKey(result.error.kind);
    } catch {
      error.value = errorKey("logs");
    }
  }

  return {
    launchAtStartup,
    loaded,
    saving,
    version,
    error,
    load,
    setLaunchAtStartup,
    openLogsFolder,
  };
});
