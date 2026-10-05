<script setup lang="ts">
// Indicateur d'attente. Sans `label`, il est décoratif (le contrôle porte `aria-busy`).
// Mouvement réduit : l'animation est coupée (base.css), l'arc reste visible.
withDefaults(defineProps<{ label?: string }>(), { label: undefined });
</script>

<template>
  <svg
    class="spinner"
    viewBox="0 0 24 24"
    fill="none"
    :role="label ? 'status' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : 'true'"
  >
    <circle class="spinner__track" cx="12" cy="12" r="9" />
    <path class="spinner__arc" d="M12 3a9 9 0 0 1 9 9" />
  </svg>
</template>

<style scoped>
.spinner {
  display: block;
  flex: none;
  width: var(--spinner-size);
  height: var(--spinner-size);
  animation: spin var(--motion-pulse) linear infinite;
}

.spinner__track {
  stroke: currentColor;
  stroke-width: var(--spinner-stroke);
  opacity: var(--opacity-pulse-low);
}

.spinner__arc {
  stroke: currentColor;
  stroke-width: var(--spinner-stroke);
  stroke-linecap: round;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
