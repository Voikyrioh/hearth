<script setup lang="ts">
import { ref, useId } from "vue";

// Bulle d'explication au survol ET au focus clavier. Le slot reçoit `describedby` à poser
// sur le contrôle pour que les lecteurs d'écran la lisent. Échap la ferme.
defineProps<{ text: string }>();

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
    <slot :describedby="id" />
    <span v-show="open" :id="id" class="tip__bubble" role="tooltip">{{ text }}</span>
  </span>
</template>

<style scoped>
.tip {
  position: relative;
  display: inline-flex;
}

.tip__bubble {
  position: absolute;
  bottom: calc(100% + var(--space-2));
  left: 50%;
  z-index: var(--z-tooltip);
  width: max-content;
  max-width: var(--tooltip-max);
  padding: var(--space-2) var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
  font-size: var(--fs-small);
  transform: translateX(-50%);
  pointer-events: none;
}
</style>
