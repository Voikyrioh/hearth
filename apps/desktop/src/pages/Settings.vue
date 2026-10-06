<script setup lang="ts">
import { computed, onMounted } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HToggle from "@/components/atoms/HToggle.vue";
import SettingRow from "@/components/molecules/SettingRow.vue";
import AgentUpdateCard from "@/components/organisms/AgentUpdateCard.vue";
import OwnAccountCard from "@/components/organisms/OwnAccountCard.vue";
import UpdatePanel from "@/components/organisms/UpdatePanel.vue";
import { t } from "@/i18n";
import { useServersStore } from "@/stores/servers";
import { useSettingsStore } from "@/stores/settings";

// Réglages, dans l'ordre du design (design-ecrans-socle.md) : à gauche « Général », « Client »,
// « Mises à jour » ; à droite, « État du serveur : {nom} » (versions, mise à jour de l'agent, HRT-17),
// une carte par serveur enregistré, puis « Mon compte », une carte par serveur (le design y met l'état de chaque
// serveur ; le compte de l'utilisateur sur ce serveur en est le prolongement naturel, et c'est la
// seule place accessible au rôle Lecture seule, qui n'a pas la page Comptes). « Notifications » est
// une préférence du CLIENT : elle vit dans la section « Client », avec la version.
const settings = useSettingsStore();
const servers = useServersStore();

onMounted(() => settings.load());

const toggleHint = computed(() => (settings.loaded ? undefined : t("settings.toggleUnavailable")));
</script>

<template>
  <main class="settings">
    <h1 class="settings__title">{{ t("settings.title") }}</h1>
    <div class="settings__columns">
      <div class="settings__column">
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
        </section>
        <section class="settings__panel">
          <h2 class="settings__section">{{ t("settings.sectionClient") }}</h2>
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
      </div>
      <div v-if="servers.servers.length > 0" class="settings__column">
        <AgentUpdateCard v-for="server in servers.servers" :key="server.id" :server="server" />
        <h2 class="settings__section settings__section--column">{{ t("settings.sectionAccount") }}</h2>
        <OwnAccountCard v-for="server in servers.servers" :key="server.id" :server="server" />
      </div>
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
  font-weight: var(--fw-semibold);
}

.settings__columns {
  display: grid;
  grid-template-columns: minmax(0, var(--panel-max)) minmax(0, 1fr);
  gap: var(--space-5);
  align-items: start;
}

.settings__column {
  min-width: 0;
}

.settings__panel {
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

.settings__section--column {
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
