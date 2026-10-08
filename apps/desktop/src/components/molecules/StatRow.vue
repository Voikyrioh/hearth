<script setup lang="ts">
import type { Level } from "@/link";
import HMiddleText from "../atoms/HMiddleText.vue";
import LevelBadge from "./LevelBadge.vue";

// Une ligne « libellé : valeur » d'une carte, avec la marque d'alerte de la valeur. Une valeur
// absente est dite en clair par l'appelant (« Non disponible »), jamais remplacée par un zéro.
withDefaults(
  defineProps<{
    label: string;
    value: string;
    level?: Level;
    muted?: boolean;
    stacked?: boolean;
    /** Pastille de la couleur de la courbe correspondante (la couleur ne porte jamais seule le sens). */
    swatch?: "ac" | "cool";
  }>(),
  { level: "normal", muted: false, stacked: false, swatch: undefined },
);
</script>

<template>
  <div :class="['stat', { 'stat--stacked': stacked }]" :data-level="level">
    <dt class="stat__label">
      <i v-if="swatch" :class="['stat__swatch', `stat__swatch--${swatch}`]" aria-hidden="true" /><HMiddleText class="stat__text" :text="label" />
    </dt>
    <dd :class="['stat__value', { 'stat__value--muted': muted }]">
      <slot>{{ value }}</slot><LevelBadge :level="level" />
    </dd>
  </div>
</template>

<style scoped>
.stat {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
  min-width: 0;
}

.stat--stacked {
  flex-direction: column;
  align-items: stretch;
  gap: var(--space-half);
}

.stat--stacked .stat__value {
  justify-content: flex-start;
  text-align: left;
}

.stat__swatch {
  display: inline-block;
  width: var(--pill-dot);
  height: var(--pill-dot);
  margin-right: var(--space-2);
  border-radius: 50%;
}

.stat__swatch--ac {
  background: var(--ac);
}

.stat__swatch--cool {
  background: var(--cool);
}

.stat__text {
  flex: 0 1 auto;
  min-width: 0;
}

.stat__label {
  display: flex;
  flex: 0 1 auto;
  align-items: center;
  min-width: 0;
  color: var(--tx2);
}

.stat__value {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-2);
  min-width: 0;
  margin: 0;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
  text-align: right;
}

/* Une valeur (« 36 °C ») ne s'écrit jamais en colonne : c'est le libellé qui se tronque (HRT-47, S1b). */
.stat:not(.stat--stacked) .stat__value {
  flex: none;
  white-space: nowrap;
}

.stat__value--muted {
  color: var(--tx3);
  font-family: var(--font-body);
}
</style>
