<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import ResourceSliders from "@/components/organisms/ResourceSliders.vue";
import type { FicheInstallState } from "@/composables/useFicheInstall";
import { diskText, memoryNoticeText, proofShort } from "@/fiche/messages";
import { proofOf, proven } from "@/fiche/proof";
import { t } from "@/i18n";
import { formatCpu, formatMemory, type Resources } from "@/resources";
import { formatSize } from "@/serviceOps/messages";

// Les « Ressources » de l'installation d'un serveur de jeu (HRT-102, BR-FICHE-020 à 023, BR-RES-037) : les deux curseurs de
// mémoire et de processeur, déjà placés sur ce que la fiche RECOMMANDE (jamais sur un chiffre sans preuve : sans preuve,
// la mémoire part de la valeur proposée pour une application inconnue et le processeur de tous les processeurs du
// serveur, et la page le dit). Il n'existe aucune position « sans limite ». Les avertissements chiffrés sont en direct :
// ce qui reste de mémoire, une limite sous le minimum ou au-dessus de ce qui reste ; les cases « Je comprends »
// correspondantes sont dans la fenêtre « Vérifiez avant de lancer », pas ici. L'espace disque : un besoin MESURÉ qui
// dépasse la place libre est une erreur sans case, un chiffre de l'éditeur un avertissement (la case est dans la fenêtre).
const props = defineProps<{ install: FicheInstallState }>();

const detail = computed(() => props.install.detail.value);
const draft = computed(() => props.install.draft.value);
const placed = computed(() => props.install.placed.value);

const resources = computed<Resources>({
  get: () => draft.value?.resources ?? { memoryBytes: null, cpuMillis: null },
  set: (value) => props.install.setResources(value),
});

/** La mémoire recommandée de la fiche et sa preuve, pour la ligne « Recommandé » du curseur. */
const recommended = computed(() => {
  const place = placed.value;
  if (!place || place.memory.recommended === null || place.memory.proof === "unmeasured") {
    return null;
  }
  return { bytes: place.memory.recommended, proof: place.memory.proof };
});

const memoryNote = computed(() => {
  const place = placed.value;
  if (recommended.value !== null || !place || place.resources.memoryBytes === null) return null;
  return t("fiche.memoryUnproven", { value: formatMemory(place.resources.memoryBytes) });
});

const cpuNote = computed(() => {
  const place = placed.value;
  if (!place) return null;
  return place.cpu.recommended === null
    ? t("fiche.resourcesCpuUnknown")
    : t("fiche.resourcesCpuRecommended", {
        value: formatCpu(place.cpu.recommended),
        proof: proofShort(place.cpu.proof),
      });
});

const minimum = computed(() => {
  const memory = detail.value?.resources.memory;
  if (!memory) return null;
  const value = proven(memory.minBytes, memory.evidence);
  return value === null
    ? null
    : t("fiche.resourcesMinimum", {
        value: formatMemory(value),
        proof: proofShort(proofOf(memory.evidence)),
      });
});

const clamped = computed(() => {
  const place = placed.value;
  return place?.memory.clamped && place.memory.recommended !== null
    ? t("fiche.resourcesClamped", { value: formatMemory(place.memory.recommended) })
    : null;
});

const disk = computed(() => diskText(props.install.disk.value));
const diskTone = computed(() =>
  props.install.disk.value.kind === "short_measured" ? "error" : "warn",
);
const free = computed(() => props.install.check.value?.disk?.freeBytes ?? null);
</script>

<template>
  <div v-if="detail && draft" class="resources" data-install-resources>
    <DashCard :title="t('fiche.resourcesTitle')" data-resources>
      <ResourceSliders
        v-model="resources"
        :capacity="install.capacity.value"
        :recommended="recommended"
        :memory-note="memoryNote"
        :cpu-note="cpuNote"
      />
      <HButton
        v-if="install.capacity.value === null && install.capacityFailed.value"
        variant="secondary"
        size="sm"
        data-capacity-retry
        @click="install.loadCapacity()"
      >
        {{ t("common.retry") }}
      </HButton>
      <p v-if="minimum" class="resources__line" data-resources-minimum>{{ minimum }}</p>
      <p v-if="clamped" class="resources__line" data-resources-clamped>{{ clamped }}</p>
      <Callout
        v-for="notice in install.notices.value"
        :key="notice.kind"
        tone="warn"
        inline
        :data-resources-notice="notice.kind"
      >
        {{ memoryNoticeText(notice) }}
      </Callout>
      <p v-if="free !== null" class="resources__line" data-resources-free>
        {{ t("fiche.diskFree", { size: formatSize(free) }) }}
      </p>
      <Callout
        v-if="disk"
        :tone="diskTone"
        :role="diskTone === 'error' ? 'alert' : 'status'"
        data-resources-disk
      >
        {{ disk }}
      </Callout>
    </DashCard>
  </div>
</template>

<style scoped>
.resources {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.resources__line {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
