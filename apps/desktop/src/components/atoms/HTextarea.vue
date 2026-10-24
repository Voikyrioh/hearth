<script setup lang="ts">
import { computed, useId } from "vue";

// Zone de texte : même langage que `HInput` (libellé visible, aide, erreur sous le champ, focus braise). Les lignes
// grandissent avec la saisie jusqu'à `maxRows`, puis la zone défile. Sert à la commande de lancement d'un service
// (texte sensible : jamais affiché ailleurs que dans ce champ et le récapitulatif).
const props = withDefaults(
  defineProps<{
    modelValue: string;
    label: string;
    help?: string;
    error?: string;
    placeholder?: string;
    disabled?: boolean;
    mono?: boolean;
    rows?: number;
    maxRows?: number;
  }>(),
  {
    help: undefined,
    error: undefined,
    placeholder: undefined,
    disabled: false,
    mono: false,
    rows: 3,
    maxRows: 8,
  },
);

defineEmits<{ "update:modelValue": [value: string]; blur: [] }>();

const id = useId();
const helpId = `${id}-help`;
const errorId = `${id}-error`;
const describedBy = computed(() => {
  const ids = [props.error ? errorId : null, props.help ? helpId : null].filter(Boolean);
  return ids.length > 0 ? ids.join(" ") : undefined;
});
/** Le nombre de lignes montrées : celles de la saisie, entre `rows` et `maxRows`. */
const shownRows = computed(() => {
  const lines = props.modelValue.split("\n").length;
  return Math.min(props.maxRows, Math.max(props.rows, lines));
});
</script>

<template>
  <div class="field">
    <label class="field__label" :for="id">{{ label }}</label>
    <div :class="['field__box', { 'field__box--error': error, 'field__box--disabled': disabled }]">
      <textarea
        :id="id"
        :class="['field__input', { 'field__input--mono': mono }]"
        :value="modelValue"
        :rows="shownRows"
        :placeholder="placeholder"
        :disabled="disabled"
        :aria-invalid="error ? 'true' : undefined"
        :aria-describedby="describedBy"
        spellcheck="false"
        @input="$emit('update:modelValue', ($event.target as HTMLTextAreaElement).value)"
        @blur="$emit('blur')"
      />
    </div>
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
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.field__box {
  display: flex;
  min-width: 0;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  transition:
    border-color var(--motion-fast) var(--ease),
    box-shadow var(--motion-fast) var(--ease);
}

.field__box:focus-within {
  border-color: var(--ac);
  box-shadow: inset 0 0 0 var(--halo-width) var(--focus-halo);
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
  max-height: var(--command-max-height);
  padding: var(--field-pad-y) var(--field-pad-x);
  border: 0;
  background: transparent;
  color: var(--tx);
  font: inherit;
  line-height: var(--lh-body);
  outline: none;
  resize: vertical;
}

.field__input--mono {
  font-family: var(--font-mono);
}

.field__input::placeholder {
  color: var(--tx3);
}

.field__error {
  color: var(--crit);
  font-size: var(--fs-small);
}

.field__help {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
