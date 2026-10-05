<script setup lang="ts">
import type { Level } from "@/link";
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
  }>(),
  { level: "normal", muted: false, stacked: false },
);
</script>

<template>
  <div :class="['stat', { 'stat--stacked': stacked }]" :data-level="level">
    <dt class="stat__label">{{ label }}</dt>
    <dd :class="['stat__value', { 'stat__value--muted': muted }]">
      {{ value }}<LevelBadge :level="level" />
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

.stat__label {
  flex: none;
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

.stat__value--muted {
  color: var(--tx3);
  font-family: var(--font-body);
}
</style>
