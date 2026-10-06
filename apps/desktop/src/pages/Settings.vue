<script setup lang="ts">
import { computed, onMounted } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HToggle from "@/components/atoms/HToggle.vue";
import SettingRow from "@/components/molecules/SettingRow.vue";
import UpdatePanel from "@/components/organisms/UpdatePanel.vue";
import { t } from "@/i18n";
import { useSettingsStore } from "@/stores/settings";

const settings = useSettingsStore();

onMounted(() => settings.load());

const toggleHint = computed(() => (settings.loaded ? undefined : t("settings.toggleUnavailable")));
</script>

<template>
  <main class="settings">
    <h1 class="settings__title">{{ t("settings.title") }}</h1>
    <section class="settings__panel">
      <h2 class="settings__section">{{ t("settings.sectionGeneral") }}</h2>
      <p v-if="settings.error" class="settings__error" role="alert">{{ t(settings.error) }}</p>
      <SettingRow
        :label="t('settings.launchAtStartup')"
        :help="t('settings.launchAtStartupHelp')"
        v-slot="{ labelId }"
      >
        <HToggle
          :aria-labelledby="labelId"
          :model-value="settings.launchAtStartup"
          :disabled="!settings.loaded"
          :busy="settings.saving"
          :hint="toggleHint"
          @update:model-value="settings.setLaunchAtStartup"
        />
      </SettingRow>
      <SettingRow :label="t('settings.logs')" :help="t('settings.logsHelp')">
        <HButton variant="secondary" @click="settings.openLogsFolder">
          {{ t("settings.openLogs") }}
        </HButton>
      </SettingRow>
      <p v-if="settings.logsError" class="settings__error" role="alert">{{ t(settings.logsError) }}</p>
      <h2 class="settings__section settings__section--spaced">
        {{ t("settings.sectionNotifications") }}
      </h2>
      <SettingRow
        :label="t('settings.notifyOnLinkChange')"
        :help="t('settings.notifyOnLinkChangeHelp')"
        v-slot="{ labelId }"
      >
        <HToggle
          :aria-labelledby="labelId"
          :model-value="settings.notifyOnLinkChange"
          :disabled="!settings.loaded"
          :busy="settings.saving"
          :hint="toggleHint"
          @update:model-value="settings.setNotifyOnLinkChange"
        />
      </SettingRow>
      <p class="settings__version">
        {{ t("settings.version") }}
        <span class="settings__mono">{{
          settings.version ?? t("settings.versionUnavailable")
        }}</span>
      </p>
    </section>
    <UpdatePanel />
  </main>
</template>

<style scoped>
.settings {
  height: 100%;
  padding: var(--space-5);
  overflow-y: auto;
}

.settings__title {
  font-size: var(--fs-h1);
  font-weight: var(--fw-semibold);
}

.settings__panel {
  max-width: var(--panel-max);
  margin-top: var(--space-5);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.settings__section {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.settings__section--spaced {
  margin-top: var(--space-5);
}

.settings__error {
  margin-top: var(--space-3);
  color: var(--crit);
}

.settings__version {
  margin-top: var(--space-4);
  padding-top: var(--space-4);
  border-top: var(--border-width) solid var(--bd);
  color: var(--tx2);
}

.settings__mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  color: var(--tx);
}
</style>
