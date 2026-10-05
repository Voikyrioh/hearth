<script setup lang="ts">
import { ref } from "vue";
import { t } from "@/i18n";
import HButton from "./HButton.vue";
import HIcon from "./HIcon.vue";
import HInput from "./HInput.vue";

// Mot de passe : masqué par défaut, bouton « Afficher / Masquer » (jamais affiché ailleurs).
withDefaults(
  defineProps<{
    modelValue: string;
    label: string;
    help?: string;
    error?: string;
    placeholder?: string;
    autocomplete?: string;
    disabled?: boolean;
  }>(),
  {
    help: undefined,
    error: undefined,
    placeholder: undefined,
    autocomplete: "off",
    disabled: false,
  },
);

defineEmits<{ "update:modelValue": [value: string] }>();

const shown = ref(false);
</script>

<template>
  <HInput
    :model-value="modelValue"
    :label="label"
    :type="shown ? 'text' : 'password'"
    :help="help"
    :error="error"
    :placeholder="placeholder"
    :autocomplete="autocomplete"
    :disabled="disabled"
    @update:model-value="$emit('update:modelValue', $event)"
  >
    <template #suffix>
      <HButton
        variant="ghost"
        size="sm"
        :aria-pressed="shown"
        :aria-label="shown ? t('field.hidePassword') : t('field.showPassword')"
        @click="shown = !shown"
      >
        <HIcon :name="shown ? 'eye-off' : 'eye'" size="sm" />
      </HButton>
    </template>
  </HInput>
</template>
