import { defineStore } from "pinia";
import { ref } from "vue";
import { commands } from "@/bindings";
import type { MessageKey } from "@/i18n";

/** Réglages locaux, lus et écrits uniquement par les commandes typées du cœur Rust. */
export const useSettingsStore = defineStore("settings", () => {
  const launchAtStartup = ref(false);
  const version = ref("");
  const loaded = ref(false);
  const error = ref<MessageKey | null>(null);

  async function load() {
    try {
      const settings = await commands.getSettings();
      if (settings.status === "ok") {
        launchAtStartup.value = settings.data.launchAtStartup;
        loaded.value = true;
        error.value = null;
      } else {
        error.value = "settings.loadError";
      }
      version.value = await commands.getAppVersion();
    } catch {
      // Hors de l'application (navigateur de revue sans pont) ou pont en panne.
      error.value = "settings.loadError";
    }
  }

  async function setLaunchAtStartup(enabled: boolean) {
    try {
      const result = await commands.setLaunchAtStartup(enabled);
      if (result.status === "ok") {
        launchAtStartup.value = result.data.launchAtStartup;
        error.value = null;
        return;
      }
    } catch {
      // Traité comme un échec d'écriture ci-dessous.
    }
    error.value = "settings.saveError";
  }

  return { launchAtStartup, version, loaded, error, load, setLaunchAtStartup };
});
