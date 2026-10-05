<script setup lang="ts">
import { computed, useId } from "vue";
import type { Level } from "@/link";

// Arc de jauge de 270° (SVG pur) : piste, puis arc de valeur qui glisse vers sa nouvelle valeur.
// Normal = dégradé braise ; attention et critique changent de couleur (le pictogramme et le
// libellé sont portés par `Gauge`, jamais la couleur seule). `ratio` nul : piste seule.
const props = withDefaults(defineProps<{ ratio: number | null; level?: Level }>(), {
  level: "normal",
});

const gradient = useId();
const dash = computed(() => {
  const ratio = props.ratio === null ? 0 : Math.min(1, Math.max(0, props.ratio));
  return `${(ratio * 100).toFixed(2)} 100`;
});

// Arc de 270° centré en (50, 50), rayon 40, de 135° à 45° dans le sens horaire.
const ARC = "M 21.72 78.28 A 40 40 0 1 1 78.28 78.28";
</script>

<template>
  <svg class="arc" viewBox="0 0 100 100" aria-hidden="true">
    <defs>
      <linearGradient :id="gradient" x1="0" y1="1" x2="1" y2="0">
        <stop offset="0" class="arc__from" />
        <stop offset="1" class="arc__to" />
      </linearGradient>
    </defs>
    <path class="arc__track" :d="ARC" pathLength="100" />
    <path
      v-if="ratio !== null"
      :class="['arc__value', `arc__value--${level}`]"
      :d="ARC"
      pathLength="100"
      :stroke-dasharray="dash"
      :stroke="level === 'normal' ? `url(#${gradient})` : undefined"
    />
  </svg>
</template>

<style scoped>
.arc {
  display: block;
  width: 100%;
  height: 100%;
}

.arc__from {
  stop-color: var(--ac2);
}

.arc__to {
  stop-color: var(--ac);
}

.arc__track,
.arc__value {
  fill: none;
  stroke-width: 12;
  stroke-linecap: round;
}

.arc__track {
  stroke: var(--bd);
}

.arc__value {
  transition: stroke-dasharray var(--motion-base) var(--ease);
}

.arc__value--normal {
  filter: drop-shadow(0 0 var(--glow-radius) color-mix(in srgb, var(--ac) 70%, transparent));
}

.arc__value--attention {
  stroke: var(--warn);
  filter: drop-shadow(0 0 var(--glow-radius) color-mix(in srgb, var(--warn) 70%, transparent));
}

.arc__value--critical {
  stroke: var(--crit);
  filter: drop-shadow(0 0 var(--glow-radius) color-mix(in srgb, var(--crit) 70%, transparent));
}
</style>
