<script setup lang="ts">
import { computed, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import ReleaseNotesDialog from "@/components/molecules/ReleaseNotesDialog.vue";
import { formatAgo } from "@/composables/formatAgo";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import { useUpdatesStore } from "@/stores/updates";

// Section « Mises à jour » des réglages : état (à jour, version disponible), « Dernière vérification »
// (BR-UPDATE-007, jamais un message d'erreur sans Internet), « Vérifier maintenant » (BR-UPDATE-026)
// et, si une version est connue, « Notes de version » et « Mettre à jour maintenant ».
const updates = useUpdatesStore();
const now = useNow();
const notesOpen = ref(false);

const state = computed(() => updates.state);
const release = computed(() => updates.available);
const checking = computed(() => updates.checking || state.value?.phase === "checking");

const status = computed(() => {
  if (release.value) return t("updates.availableVersion", { version: release.value.version });
  if (state.value?.upToDate) return t("updates.upToDate");
  return null;
});

const lastCheck = computed(() => {
  const at = state.value?.lastCheckedAt ?? null;
  return at === null
    ? t("updates.neverChecked")
    : t("updates.lastCheck", { when: formatAgo(at, now.value) });
});
</script>

<template>
  <section class="updates" data-updates-panel>
    <h2 class="updates__title">{{ t("updates.sectionTitle") }}</h2>
    <p v-if="status" class="updates__status" data-updates-status>{{ status }}</p>
    <p class="updates__last" data-updates-last>{{ lastCheck }}</p>
    <p class="updates__help">{{ t("updates.checkHelp") }}</p>
    <div class="updates__actions">
      <HButton variant="secondary" :busy="checking" :disabled="updates.busy" @click="updates.checkNow()">
        {{ checking ? t("updates.checking") : t("updates.checkNow") }}
      </HButton>
      <template v-if="release">
        <HButton variant="ghost" @click="notesOpen = true">{{ t("updates.notes") }}</HButton>
        <HButton :busy="updates.busy" @click="updates.install()">{{ t("updates.updateNow") }}</HButton>
      </template>
    </div>
    <ReleaseNotesDialog
      v-if="release"
      :open="notesOpen"
      :version="release.version"
      :notes="release.notes"
      @close="notesOpen = false"
    />
  </section>
</template>

<style scoped>
.updates {
  max-width: var(--panel-max);
  margin-top: var(--space-5);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.updates__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.updates__status {
  margin-top: var(--space-3);
  font-weight: var(--fw-medium);
}

.updates__last {
  margin-top: var(--space-1);
  color: var(--tx2);
}

.updates__help {
  margin-top: var(--space-1);
  color: var(--tx3);
}

.updates__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin-top: var(--space-4);
}
</style>
