<script setup lang="ts">
import { onMounted } from "vue";
import HToggle from "@/components/atoms/HToggle.vue";
import { t } from "@/i18n";
import { useSettingsStore } from "@/stores/settings";

const settings = useSettingsStore();

onMounted(() => settings.load());
</script>

<template>
  <main class="settings">
    <h1 class="settings__title">{{ t("settings.title") }}</h1>
    <div class="settings__layout">
      <nav class="settings__tabs" :aria-label="t('settings.tabsLabel')">
        <span class="settings__tab" aria-current="page">{{ t("settings.tabGeneral") }}</span>
      </nav>
      <section class="settings__panel">
        <h2 class="settings__section">{{ t("settings.tabGeneral") }}</h2>
        <p v-if="settings.error" class="settings__error" role="alert">{{ t(settings.error) }}</p>
        <div class="settings__row">
          <div class="settings__text">
            <span id="launch-at-startup-label" class="settings__label">
              {{ t("settings.launchAtStartup") }}
            </span>
            <p class="settings__help">{{ t("settings.launchAtStartupHelp") }}</p>
          </div>
          <HToggle
            aria-labelledby="launch-at-startup-label"
            :model-value="settings.launchAtStartup"
            :disabled="!settings.loaded"
            @update:model-value="settings.setLaunchAtStartup"
          />
        </div>
        <p v-if="settings.version" class="settings__version">
          {{ t("settings.version") }} <span class="settings__mono">{{ settings.version }}</span>
        </p>
      </section>
    </div>
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
  font-weight: 600;
}

.settings__layout {
  display: grid;
  grid-template-columns: 208px minmax(0, 640px);
  gap: var(--space-5);
  margin-top: var(--space-5);
}

.settings__tabs {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.settings__tab {
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--card);
  font-weight: 500;
}

.settings__panel {
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.settings__section {
  font-size: var(--fs-h3);
  font-weight: 600;
}

.settings__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-5);
  margin-top: var(--space-4);
}

.settings__label {
  font-weight: 500;
}

.settings__help {
  margin-top: var(--space-1);
  color: var(--tx2);
}

.settings__error {
  margin-top: var(--space-3);
  color: var(--crit);
}

.settings__version {
  margin-top: var(--space-5);
  padding-top: var(--space-4);
  border-top: 1px solid var(--bd);
  color: var(--tx2);
}

.settings__mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  color: var(--tx);
}
</style>
