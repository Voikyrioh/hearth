<script setup lang="ts">
// Désactivé = `aria-disabled` (et non `disabled`) : le bouton garde le focus clavier
// et son infobulle, qui explique pourquoi il est inactif.
const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost";
    disabled?: boolean;
    hint?: string;
    type?: "button" | "submit";
  }>(),
  { variant: "primary", disabled: false, hint: undefined, type: "button" },
);

const emit = defineEmits<{ click: [event: MouseEvent] }>();

function onClick(event: MouseEvent) {
  if (props.disabled) {
    event.preventDefault();
    return;
  }
  emit("click", event);
}
</script>

<template>
  <button
    :type="type"
    :class="['btn', `btn--${variant}`, { 'btn--disabled': disabled }]"
    :aria-disabled="disabled ? 'true' : undefined"
    :title="hint"
    @click="onClick"
  >
    <slot />
  </button>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  min-height: 40px;
  padding: 0 var(--space-4);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  font-weight: 600;
  cursor: pointer;
  transition:
    background var(--motion-fast) var(--ease),
    border-color var(--motion-fast) var(--ease);
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

.btn--ghost {
  min-width: 40px;
  padding: 0;
  background: transparent;
  color: var(--tx2);
}

.btn--ghost:hover {
  background: var(--card-2);
  color: var(--tx);
}

.btn--disabled,
.btn--disabled:hover {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn--primary.btn--disabled:hover {
  background: var(--ac);
}
</style>
