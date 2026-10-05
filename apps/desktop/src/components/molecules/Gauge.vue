<script setup lang="ts">
import { t } from "@/i18n";
import type { Level } from "@/link";
import HGaugeArc from "../atoms/HGaugeArc.vue";
import LevelBadge from "./LevelBadge.vue";

// Jauge : arc de 270°, valeur au centre (mono), libellé dessous, marque d'alerte. Une mesure
// illisible (`ratio` nul) montre « Non disponible » à la place de la valeur (BR-DASH-008).
withDefaults(
  defineProps<{
    label: string;
    ratio: number | null;
    valueText: string;
    level?: Level;
    size?: "md" | "sm";
  }>(),
  { level: "normal", size: "md" },
);
</script>

<template>
  <figure
    :class="['gauge', `gauge--${size}`]"
    :aria-label="t('dash.gaugeLabel', { name: label, value: ratio === null ? t('dash.unavailable') : valueText })"
    :data-level="level"
  >
    <div class="gauge__dial">
      <HGaugeArc :ratio="ratio" :level="level" />
      <span :class="['gauge__value', { 'gauge__value--missing': ratio === null }]">{{
        ratio === null ? t("dash.unavailable") : valueText
      }}</span>
    </div>
    <figcaption class="gauge__caption">
      <span class="gauge__label">{{ label }}</span>
      <LevelBadge :level="level" />
    </figcaption>
  </figure>
</template>

<style scoped>
.gauge {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
}

.gauge__dial {
  position: relative;
  width: var(--gauge-size);
  height: var(--gauge-size);
}

.gauge--sm .gauge__dial {
  width: var(--gauge-size-sm);
  height: var(--gauge-size-sm);
}

.gauge__value {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 var(--space-4);
  font-family: var(--font-mono);
  font-size: var(--fs-h3);
  font-variant-numeric: tabular-nums;
  text-align: center;
}

.gauge__value--missing {
  color: var(--tx3);
  font-family: var(--font-body);
  font-size: var(--fs-small);
}

.gauge__caption {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-1);
  color: var(--tx2);
  font-size: var(--fs-small);
}
</style>
