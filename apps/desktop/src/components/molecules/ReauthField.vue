<script setup lang="ts">
import { ref } from "vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import { t } from "@/i18n";

// Le champ « Ton mot de passe » de la confirmation d'un acte d'administration (HRT-30). Un seul motif
// pour tous les actes : masqué, `current-password` (jamais mémorisé par le navigateur : aucune valeur
// n'est gardée ici), aide sous le champ, erreur sous le champ (mot de passe faux, attente). Vidé par la
// fenêtre qui le porte après CHAQUE envoi.
withDefaults(
  defineProps<{
    modelValue: string;
    label?: string;
    help?: string;
    error?: string;
  }>(),
  { label: undefined, help: undefined, error: undefined },
);

defineEmits<{ "update:modelValue": [value: string] }>();

const root = ref<HTMLElement | null>(null);

/** Met le curseur dans le champ (à l'ouverture d'une fenêtre qui n'a pas d'autre champ). */
function focus() {
  root.value?.querySelector("input")?.focus();
}

defineExpose({ focus });
</script>

<template>
  <div ref="root" class="reauth" data-reauth-field>
    <HPasswordInput
      :model-value="modelValue"
      :label="label ?? t('reauth.password')"
      :placeholder="t('reauth.passwordPlaceholder')"
      :help="help"
      :error="error"
      autocomplete="current-password"
      @update:model-value="$emit('update:modelValue', $event)"
    />
  </div>
</template>

<style scoped>
.reauth {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
</style>
