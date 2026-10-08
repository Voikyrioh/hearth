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

// Hauteur de la ligne « durée / échelle », posée PAR-DESSUS le haut de la courbe (elle n'ajoute aucune hauteur à la carte).
const HEAD_PX = 18;

const SPAN_KEYS: Record<WindowKey, MessageKey> = {
  "1m": "dash.span1m",
  "5m": "dash.span5m",
  "1h": "dash.span1h",
};
const span = computed(() => t(SPAN_KEYS[props.window]));
const SHORT_KEYS: Record<WindowKey, MessageKey> = {
  "1m": "dash.window1m",
  "5m": "dash.window5m",
  "1h": "dash.window1h",
};
const spanShort = computed(() => t(SHORT_KEYS[props.window]));
const scale = computed(() =>
  t("dash.scaleTo", { max: props.format(ceilingOf(props.series, props.max, props.atLeast)) }),
);
// FIX:01M4E9T718D37EXTXMWA7YXWJE (C10)
// Texte équivalent de la courbe : sa dernière valeur, son minimum et son maximum sur la fenêtre.
const described = computed(() => {
  const summaries = props.series.flatMap((serie) => {
    const summary = summaryOf(serie.points);
    return summary ? [{ name: serie.name, summary }] : [];
  });
  const [only] = summaries;
  if (!only) return props.label;
  const fields = (summary: NonNullable<typeof only>["summary"]) => ({
    last: props.format(summary.last),
    min: props.format(summary.min),
    max: props.format(summary.max),
  });
  if (summaries.length === 1 && !only.name) {
    return `${props.label}. ${t("dash.chartSummary", { ...fields(only.summary), span: span.value })}`;
  }
  // Plusieurs séries : le résumé de chacune, nommée (le montant ET le descendant du réseau).
  const parts = summaries.map(({ name, summary }) =>
    t("dash.chartSummaryNamed", { name: name ?? "", ...fields(summary) }),
  );
  return `${props.label}. ${parts.join(" ; ")}, ${t("dash.onSpan", { span: span.value })}`;
});

const partial = computed(() => props.coveredMs < WINDOWS[props.window].spanMs - 5000);
</script>

<template>
  <div class="series">
    <div class="series__head">
      <p v-if="legend" class="series__legend">{{ legend }}</p>
      <div class="series__line">
        <span class="series__span">
          <span class="series__span-full">{{ span }}</span>
          <span class="series__span-short" aria-hidden="true">{{ spanShort }}</span>
        </span>
        <span class="series__scale">{{ scale }}</span>
      </div>
    </div>
    <HAreaChart
      :series="series"
      :max="max"
      :at-least="atLeast"
      :label="described"
      :format="format"
      :reserve-top="legend ? HEAD_PX * 2 : HEAD_PX"
    />
    <p v-if="partial" class="series__covered">
      {{ t("dash.coveredSince", { duration: formatCovered(coveredMs) }) }}
    </p>
  </div>
</template>

<style scoped>
.series {
  container: series / inline-size;
  position: relative;
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
  position: absolute;
  top: 0;
  right: 0;
  left: 0;
  z-index: 1;
  pointer-events: none;
  color: var(--tx3);
  font-size: var(--fs-small);
}

.series__line {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
}

.series__span,
.series__scale {
  white-space: nowrap;
}

/* FIX:01M4EHYP5JP846FGWCXJ41ET32 : chez une courbe étroite (fenêtre de 1 100 px), la durée s'écrit « 5 min » et reste sur une ligne. */
.series__span-short {
  display: none;
}

@container series (max-width: 260px) {
  .series__span-full {
    display: none;
  }

  .series__span-short {
    display: inline;
  }
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
