<script setup lang="ts">
import { computed } from "vue";
import HSpinner from "./HSpinner.vue";

// Désactivé = `aria-disabled` (et non `disabled`) : le bouton garde le focus clavier
// et son infobulle `hint`, qui explique pourquoi il est inactif. `busy` bloque aussi
// le clic et montre un indicateur d'attente. `solid` (destructeur seulement) : plein,
// réservé à la confirmation finale.
const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "danger" | "ghost";
    size?: "sm" | "md" | "lg";
    disabled?: boolean;
    busy?: boolean;
    solid?: boolean;
    hint?: string;
    type?: "button" | "submit";
  }>(),
  {
    variant: "primary",
    size: "md",
    disabled: false,
    busy: false,
    solid: false,
    hint: undefined,
    type: "button",
  },
);

const emit = defineEmits<{ click: [event: MouseEvent] }>();

const inert = computed(() => props.disabled || props.busy);

function onClick(event: MouseEvent) {
  if (inert.value) {
    event.preventDefault();
    return;
  }
  emit("click", event);
}
</script>

<template>
  <button
    :type="type"
    :class="[
      'btn',
      `btn--${variant}`,
      `btn--${size}`,
      { 'btn--disabled': disabled, 'btn--busy': busy, 'btn--solid': solid && variant === 'danger' },
    ]"
    :aria-disabled="inert ? 'true' : undefined"
    :aria-busy="busy ? 'true' : undefined"
    :title="hint"
    @click="onClick"
  >
    <HSpinner v-if="busy" />
    <slot />
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  min-height: var(--target-size);
  padding: 0 var(--space-4);
  border: var(--border-width) solid transparent;
  border-radius: var(--radius-control);
  font-weight: 600;
  white-space: nowrap;
  cursor: pointer;
  transition:
    background var(--motion-fast) var(--ease),
    border-color var(--motion-fast) var(--ease);
}

.btn--sm {
  min-height: var(--control-sm);
  padding: 0 var(--space-3);
  font-size: var(--fs-small);
  font-weight: 500;
}

.btn--lg {
  min-height: var(--control-lg);
  padding: 0 var(--space-5);
  font-size: var(--fs-lead);
}

.btn--primary {
  background: var(--ac);
  color: var(--on-ac);
}

.btn--primary:hover {
  background: var(--ac-hover);
}

.btn--secondary {
  border-color: var(--bd);
  background: transparent;
  color: var(--tx);
}

.btn--secondary:hover {
  background: var(--card-2);
}

.btn--danger {
  border-color: var(--crit);
  background: transparent;
  color: var(--crit);
}

.btn--danger:hover {
  background: var(--crit-hover);
}

.btn--danger.btn--solid {
  background: var(--crit);
  color: var(--on-crit);
}

.btn--ghost {
  min-width: var(--target-size);
  padding: 0;
  background: transparent;
  color: var(--tx2);
}

.btn--ghost:hover {
  background: var(--card-2);
  color: var(--tx);
}

.btn--disabled,
.btn--busy,
.btn--disabled:hover,
.btn--busy:hover {
  opacity: var(--opacity-disabled);
  cursor: not-allowed;
}

.btn--busy {
  cursor: progress;
}

.btn--primary.btn--disabled:hover,
.btn--primary.btn--busy:hover {
  background: var(--ac);
}

.btn--danger.btn--solid.btn--disabled:hover,
.btn--danger.btn--solid.btn--busy:hover {
  background: var(--crit);
}
</style>
