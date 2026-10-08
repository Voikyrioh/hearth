<script setup lang="ts">
import { computed } from "vue";
import type { Level } from "@/link";

// Arc de jauge de 270° (SVG pur) : piste, puis arc de valeur qui glisse vers sa nouvelle valeur.
// Normal = trait turquoise neutre ; attention et critique changent de couleur (le pictogramme et le
// libellé sont portés par `Gauge`, jamais la couleur seule). `ratio` nul : piste seule.
const props = withDefaults(defineProps<{ ratio: number | null; level?: Level }>(), {
  level: "normal",
});

const dash = computed(() => {
  const ratio = props.ratio === null ? 0 : Math.min(1, Math.max(0, props.ratio));
  return `${(ratio * 100).toFixed(2)} 100`;
});

// Arc de 270° centré en (50, 50), rayon 40, de 135° à 45° dans le sens horaire.
const ARC = "M 21.72 78.28 A 40 40 0 1 1 78.28 78.28";
</script>

<template>
  <svg class="arc" viewBox="0 0 100 100" aria-hidden="true">
    <path class="arc__track" :d="ARC" pathLength="100" />
    <path
      v-if="ratio !== null"
      :class="['arc__value', `arc__value--${level}`]"
      :d="ARC"
      pathLength="100"
      :stroke-dasharray="dash"
    />
  </svg>
</template>

<style scoped>
.arc {
  display: block;
  width: 100%;
  height: 100%;
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

/* FIX:01M4E9T77SYNC7MK8Q1JP5PQ4J */
/* HRT-41 (C13) : à l'état normal la jauge est neutre (turquoise) ; le rose et l'ambre sont réservés à l'attention et à l'alerte.
   Décision de Claude du 2026-10-08, à confirmer par Voiky. */
.arc__value--normal {
  stroke: var(--cool);
  filter: drop-shadow(0 0 var(--glow-radius) color-mix(in srgb, var(--cool) 70%, transparent));
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
