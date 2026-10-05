<script setup lang="ts">
import { computed, useId } from "vue";
import { type NeedsLink, useNeedsLink } from "@/composables/useNeedsLink";
import HTooltip from "./HTooltip.vue";

// Champ texte : libellé toujours visible, aide, erreur sous le champ (`aria-invalid`,
// `aria-describedby`), focus braise. Le slot `suffix` accueille une action (ex. afficher
// un mot de passe).
const props = withDefaults(
  defineProps<{
    modelValue: string;
    label: string;
    type?: "text" | "password" | "search";
    help?: string;
    error?: string;
    placeholder?: string;
    autocomplete?: string;
    disabled?: boolean;
    mono?: boolean;
    needsLink?: NeedsLink;
  }>(),
  {
    type: "text",
    help: undefined,
    error: undefined,
    placeholder: undefined,
    autocomplete: "off",
    disabled: false,
    mono: false,
    needsLink: undefined,
  },
);

defineEmits<{ "update:modelValue": [value: string] }>();

const id = useId();
const helpId = `${id}-help`;
const errorId = `${id}-error`;
// Besoin du serveur : le champ devient lecture seule (il garde le focus) et dit pourquoi.
const linkReason = useNeedsLink(() => props.needsLink);
const locked = computed(() => linkReason.value !== null);
const describedBy = computed(() => {
  const ids = [props.error ? errorId : null, props.help ? helpId : null].filter(Boolean);
  return ids.length > 0 ? ids.join(" ") : undefined;
});
</script>

<template>
  <div class="field">
    <label class="field__label" :for="id">{{ label }}</label>
    <HTooltip :text="linkReason ?? undefined" placement="start">
      <template #default="{ describedby }">
    <div :class="['field__box', { 'field__box--error': error, 'field__box--disabled': disabled || locked }]">
      <input
        :id="id"
        :class="['field__input', { 'field__input--mono': mono }]"
        :value="modelValue"
        :type="type"
        :placeholder="placeholder"
        :autocomplete="autocomplete"
        :disabled="disabled"
        :aria-invalid="error ? 'true' : undefined"
        :readonly="locked"
        :aria-disabled="locked ? 'true' : undefined"
        :aria-describedby="[describedBy, describedby].filter(Boolean).join(' ') || undefined"
        @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
      />
      <slot name="suffix" />
    </div>
      </template>
    </HTooltip>
    <p v-if="error" :id="errorId" class="field__error" role="alert">{{ error }}</p>
    <p v-if="help" :id="helpId" class="field__help">{{ help }}</p>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.field__label {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.field__box {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  transition:
    border-color var(--motion-fast) var(--ease),
    box-shadow var(--motion-fast) var(--ease);
}

.field__box:focus-within {
  border-color: var(--ac);
  box-shadow: 0 0 0 var(--halo-width) var(--focus-halo);
}

.field__box--error,
.field__box--error:focus-within {
  border-color: var(--crit);
}

.field__box--disabled {
  opacity: var(--opacity-disabled);
}

.field__input {
  flex: 1;
  min-width: 0;
  padding: var(--field-pad-y) var(--field-pad-x);
  border: 0;
  background: transparent;
  color: var(--tx);
  font: inherit;
  outline: none;
}

.field__input--mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.field__input::placeholder {
  color: var(--tx3);
}

.field__error {
  color: var(--crit);
  font-size: var(--fs-small);
}

.field__help {
  color: var(--tx2);
  font-size: var(--fs-small);
}
</style>
