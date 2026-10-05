<script setup lang="ts">
import { ref, useId } from "vue";

// Bulle d'explication au survol ET au focus clavier (`role="tooltip"`), fermée par Échap.
// Le slot reçoit `describedby` à poser sur le contrôle pour que les lecteurs d'écran la lisent.
// Sans `text`, aucune bulle (l'enveloppe reste : le contrôle n'est jamais recréé quand
// l'explication apparaît ou disparaît, le focus clavier est conservé). `placement` aligne la bulle sur le
// début, le centre ou la fin du contrôle (éviter de sortir de la fenêtre).
withDefaults(
  defineProps<{ text?: string; placement?: "start" | "center" | "end"; side?: "bottom" | "top" }>(),
  { text: undefined, placement: "center", side: "bottom" },
);

const id = useId();
const open = ref(false);
</script>

<template>
  <span
    class="tip"
    @mouseenter="open = true"
    @mouseleave="open = false"
    @focusin="open = true"
    @focusout="open = false"
    @keydown.esc="open = false"
  >
    <slot :describedby="text ? id : undefined" />
    <span
      v-if="text"
      v-show="open"
      :id="id"
      :class="['tip__bubble', `tip__bubble--${placement}`, `tip__bubble--${side}`]"
      role="tooltip"
      >{{ text }}</span
    >
  </span>
</template>

<style scoped>
.tip {
  position: relative;
  display: inline-flex;
}

.tip__bubble {
  position: absolute;
  z-index: var(--z-tooltip);
  width: max-content;
  max-width: var(--tooltip-max);
  padding: var(--space-2) var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
  font-size: var(--fs-small);
  font-weight: var(--fw-regular);
  white-space: normal;
  pointer-events: none;
}

.tip__bubble--bottom {
  top: calc(100% + var(--space-2));
}

.tip__bubble--top {
  bottom: calc(100% + var(--space-2));
}

.tip__bubble--start {
  left: 0;
}

.tip__bubble--end {
  right: 0;
}

.tip__bubble--center {
  left: 50%;
  transform: translateX(-50%);
}
</style>
