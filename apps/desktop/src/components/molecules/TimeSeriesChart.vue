<script setup lang="ts">
import { computed } from "vue";
import { formatCovered } from "@/dashboard/format";
import { ceilingOf, summaryOf, WINDOWS, type WindowKey } from "@/dashboard/series";
import type { MessageKey } from "@/i18n";
import { t } from "@/i18n";
import HAreaChart, { type ChartSeries } from "../atoms/HAreaChart.vue";

// Courbe d'une mesure sur la fenêtre choisie. Quand l'historique gardé ne couvre pas toute la
// fenêtre (application récemment ouverte), le dit : « Depuis 12 min », jamais une courbe qui
// prétend couvrir plus que ce qu'elle sait.
const props = defineProps<{
  series: readonly ChartSeries[];
  max: number | null;
  atLeast?: number;
  label: string;
  window: WindowKey;
  coveredMs: number;
  /** Met en forme une valeur avec son unité (échelle, survol, texte équivalent). */
  format: (value: number) => string;
  /** Ce que trace la courbe, quand la carte ne le dit pas déjà (disques, températures). */
  legend?: string;
}>();

const SPAN_KEYS: Record<WindowKey, MessageKey> = {
  "1m": "dash.span1m",
  "5m": "dash.span5m",
  "1h": "dash.span1h",
};
const span = computed(() => t(SPAN_KEYS[props.window]));
const scale = computed(() =>
  t("dash.scaleTo", { max: props.format(ceilingOf(props.series, props.max, props.atLeast)) }),
);
// FIX:01M4E9T718D37EXTXMWA7YXWJE (C10)
// Texte équivalent de la courbe : sa dernière valeur, son minimum et son maximum sur la fenêtre.
const described = computed(() => {
  const first = props.series[0];
  const summary = first ? summaryOf(first.points) : null;
  if (!summary) return props.label;
  return `${props.label}. ${t("dash.chartSummary", {
    last: props.format(summary.last),
    min: props.format(summary.min),
    max: props.format(summary.max),
    span: span.value,
  })}`;
});

const partial = computed(() => props.coveredMs < WINDOWS[props.window].spanMs - 5000);
</script>

<template>
  <div class="series">
    <p v-if="legend" class="series__legend">{{ legend }}</p>
    <div class="series__head">
      <span class="series__span">{{ span }}</span>
      <span class="series__scale">{{ scale }}</span>
    </div>
    <HAreaChart
      :series="series"
      :max="max"
      :at-least="atLeast"
      :label="described"
      :format="format"
    />
    <p v-if="partial" class="series__covered">
      {{ t("dash.coveredSince", { duration: formatCovered(coveredMs) }) }}
    </p>
  </div>
</template>

<style scoped>
.series {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: var(--space-1);
  min-height: 0;
}

.series__legend {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.series__head {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
  color: var(--tx3);
  font-size: var(--fs-small);
}

.series__scale {
  font-family: var(--font-mono);
}

.series__covered {
  color: var(--tx3);
  font-size: var(--fs-small);
  text-align: right;
}
</style>
