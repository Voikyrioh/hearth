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
  /** Notifications système du lien (BR-RESIL-015) : activées tant qu'on ne les a pas coupées. */
  const notifyOnLinkChange = ref(true);
  /** Alertes de sécurité (BR-TRUST-033) : réglage séparé, activé par défaut ; ne touche jamais les bandeaux. */
  const notifyOnSecurityAlert = ref(true);
  const loaded = ref(false);
  const saving = ref(false);
  const version = ref<string | null>(null);
  const error = ref<MessageKey | null>(null);
  const logsError = ref<MessageKey | null>(null);

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
      const notify = await commands.getNotifyOnLinkChange();
      if (notify.status === "ok" && typeof notify.data === "boolean") {
        notifyOnLinkChange.value = notify.data;
      }
    } catch {
      // Le réglage garde sa valeur par défaut ; l'erreur de lecture des réglages le dit déjà.
    }
    try {
      const security = await commands.getNotifyOnSecurityAlert();
      if (security.status === "ok" && typeof security.data === "boolean") {
        notifyOnSecurityAlert.value = security.data;
      }
    } catch {
      // Le réglage garde sa valeur par défaut (activé).
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

  async function setNotifyOnLinkChange(enabled: boolean) {
    if (saving.value) return;
    saving.value = true;
    try {
      const result = await commands.setNotifyOnLinkChange(enabled);
      if (result.status === "ok") {
        notifyOnLinkChange.value = result.data;
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

  async function setNotifyOnSecurityAlert(enabled: boolean) {
    if (saving.value) return;
    saving.value = true;
    try {
      const result = await commands.setNotifyOnSecurityAlert(enabled);
      if (result.status === "ok") {
        notifyOnSecurityAlert.value = result.data;
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
      logsError.value = result.status === "ok" ? null : errorKey(result.error.kind);
    } catch {
      logsError.value = errorKey("logs");
    }
  }

  return {
    launchAtStartup,
    notifyOnLinkChange,
    setNotifyOnLinkChange,
    notifyOnSecurityAlert,
    setNotifyOnSecurityAlert,
    loaded,
    saving,
    version,
    error,
    logsError,
    load,
    setLaunchAtStartup,
    openLogsFolder,
  };
});
