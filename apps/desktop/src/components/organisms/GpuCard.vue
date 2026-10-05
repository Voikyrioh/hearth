<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";
import type { ServerMachine } from "@/stores/dashboard";
import DashCard from "../molecules/DashCard.vue";
import GpuPanel from "./GpuPanel.vue";

// Carte graphique : sans carte, la section reste, avec « Non disponible sur cette machine »
// (BR-DASH-005). Plusieurs cartes : une sous-section chacune, avec son nom.
const props = defineProps<{ entry: ServerMachine }>();

const present = computed(() => props.entry.machine?.capabilities.gpu === true);
const indexes = computed(() => {
  const listed = props.entry.latest?.sample.gpus.length ?? 0;
  const known = props.entry.machine?.gpus.length ?? 0;
  return Array.from({ length: Math.max(listed, known) }, (_, index) => index);
});
</script>

<template>
  <DashCard :title="t('dash.gpu')">
    <p v-if="!present" class="gpu-card__none">{{ t("dash.notOnMachine") }}</p>
    <template v-else>
      <GpuPanel v-for="index in indexes" :key="index" :entry="entry" :index="index" />
    </template>
  </DashCard>
</template>

<style scoped>
.gpu-card__none {
  color: var(--tx2);
}
</style>
