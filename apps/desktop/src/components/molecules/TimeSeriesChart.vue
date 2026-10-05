<script setup lang="ts">
import { computed } from "vue";
import { formatCovered } from "@/dashboard/format";
import { WINDOWS, type WindowKey } from "@/dashboard/series";
import { t } from "@/i18n";
import HAreaChart, { type ChartSeries } from "../atoms/HAreaChart.vue";

// Courbe d'une mesure sur la fenêtre choisie. Quand l'historique gardé ne couvre pas toute la
// fenêtre (application récemment ouverte), le dit : « Depuis 12 min », jamais une courbe qui
// prétend couvrir plus que ce qu'elle sait.
const props = defineProps<{
  series: readonly ChartSeries[];
  max: number | null;
  label: string;
  window: WindowKey;
  coveredMs: number;
}>();

const partial = computed(() => props.coveredMs < WINDOWS[props.window].spanMs - 5000);
</script>

<template>
  <div class="series">
    <HAreaChart :series="series" :max="max" :label="label" />
    <p v-if="partial" class="series__covered">
      {{ t("dash.coveredSince", { duration: formatCovered(coveredMs) }) }}
    </p>
  </div>
</template>

<style scoped>
.series {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.series__covered {
  color: var(--tx3);
  font-size: var(--fs-small);
  text-align: right;
}
</style>
