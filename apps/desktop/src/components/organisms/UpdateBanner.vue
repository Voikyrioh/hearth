<script setup lang="ts">
import { computed, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import ReleaseNotesDialog from "@/components/molecules/ReleaseNotesDialog.vue";
import { t } from "@/i18n";
import { useUpdatesStore } from "@/stores/updates";

// Bandeau discret en haut de la fenêtre (BR-UPDATE-003) : annonce d'une version plus récente avec
// « Notes de version », « Mettre à jour maintenant » et « Plus tard » ; puis, selon l'état, le travail
// en cours (téléchargement, installation) ou l'échec (interrompu, corrompu, autre). Jamais modal, jamais
// bloquant : la version en cours reste utilisable. Il ne s'affiche que si la coquille le décide
// (`bannerVisible`), jamais d'après une date calculée ici.
const updates = useUpdatesStore();
const notesOpen = ref(false);

const kind = computed(() => updates.banner);
const release = computed(() => updates.available);
const progress = computed(() => updates.state?.progress ?? null);

const failureText = computed(() => {
  switch (updates.state?.failure) {
    case "interrupted":
      return t("updates.interrupted");
    case "corrupted":
      return t("updates.corrupted");
    default:
      return t("updates.failed");
  }
});

const downloadText = computed(() =>
  progress.value === null || progress.value <= 0
    ? t("updates.downloading")
    : t("updates.downloadingPercent", { percent: progress.value }),
);
</script>

<template>
  <div
    v-if="kind"
    class="update"
    :class="{ 'update--failed': kind === 'failed' }"
    :role="kind === 'failed' ? 'alert' : 'status'"
    :aria-label="t('updates.banner')"
    data-update-banner
    :data-state="kind"
  >
    <HIcon class="update__icon" :name="kind === 'failed' ? 'alert' : 'info'" />

    <template v-if="kind === 'available' && release">
      <p class="update__text">
        {{ t("updates.available") }}
        <span class="update__version">{{ release.version }}</span>
      </p>
      <HButton variant="ghost" size="sm" @click="notesOpen = true">{{ t("updates.notes") }}</HButton>
      <HButton variant="secondary" size="sm" @click="updates.postpone()">
        {{ t("updates.later") }}
      </HButton>
      <HButton size="sm" @click="updates.install()">{{ t("updates.updateNow") }}</HButton>
    </template>

    <template v-else-if="kind === 'downloading'">
      <HSpinner />
      <p class="update__text" data-update-progress>{{ downloadText }}</p>
    </template>

    <template v-else-if="kind === 'installing'">
      <HSpinner />
      <p class="update__text">{{ t("updates.installing") }}</p>
    </template>

    <template v-else-if="kind === 'failed'">
      <p class="update__text">{{ failureText }}</p>
      <HButton variant="secondary" size="sm" @click="updates.postpone()">
        {{ t("updates.later") }}
      </HButton>
      <HButton size="sm" @click="updates.install()">{{ t("common.retry") }}</HButton>
    </template>

    <ReleaseNotesDialog
      v-if="release"
      :open="notesOpen"
      :version="release.version"
      :notes="release.notes"
      @close="notesOpen = false"
    />
  </div>
</template>

<style scoped>
.update {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-4);
  border-bottom: var(--border-width) solid var(--bd);
  border-left: var(--ring-width) solid var(--ac);
  background: var(--card);
}

.update--failed {
  border-left-color: var(--warn);
}

.update__icon {
  flex: none;
  color: var(--ac);
}

.update--failed .update__icon {
  color: var(--warn);
}

.update__text {
  flex: 1;
  min-width: 0;
}

.update__version {
  margin-left: var(--space-2);
  color: var(--tx2);
  font-family: var(--font-mono);
}
</style>
