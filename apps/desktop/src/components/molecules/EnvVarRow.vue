<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HInput from "@/components/atoms/HInput.vue";
import { type EnvRow, valueShown } from "@/deploy/draft";
import { t } from "@/i18n";

// Une variable d'environnement du formulaire de déploiement (HRT-69, BR-DEPLOY-006) : son nom, sa valeur, un bouton
// pour montrer ou masquer la valeur, un bouton pour retirer la rangée. La valeur est masquée d'office quand le nom
// évoque un secret ; l'utilisateur peut montrer ou masquer n'importe quelle valeur à la main. Le nom et la valeur sont
// des SECRETS : jamais dans un titre, une infobulle ou une trace. Les libellés ne sont visibles que sur la première
// rangée (les suivantes les gardent pour les lecteurs d'écran). Les défauts arrivent déjà en phrases : la rangée ne
// juge rien.
defineProps<{
  row: EnvRow;
  nameError?: string;
  valueError?: string;
  /** Première rangée : porte les libellés visibles. */
  first?: boolean;
  /** Rang (à partir de 1) du bouton « Retirer » : le NOM de la variable est un secret, il ne va pas dans un nom accessible. */
  position: number;
}>();

defineEmits<{
  "update:name": [value: string];
  "update:value": [value: string];
  toggle: [];
  remove: [];
  "blur-name": [];
  "blur-value": [];
}>();
</script>

<template>
  <div :class="['env', { 'env--labelled': first }]" data-env-row :data-row-id="row.id">
    <HInput
      class="env__name"
      :model-value="row.name"
      :label="t('deploy.envNameLabel')"
      :hide-label="!first"
      :error="nameError"
      mono
      @update:model-value="$emit('update:name', $event)"
      @blur="$emit('blur-name')"
    />
    <HInput
      class="env__value"
      :model-value="row.value"
      :label="t('deploy.envValueLabel')"
      :hide-label="!first"
      :type="valueShown(row) ? 'text' : 'password'"
      autocomplete="off"
      :error="valueError"
      mono
      @update:model-value="$emit('update:value', $event)"
      @blur="$emit('blur-value')"
    >
      <template #suffix>
        <HButton
          variant="ghost"
          size="sm"
          data-env-toggle
          :aria-pressed="valueShown(row)"
          :aria-label="valueShown(row) ? t('deploy.envHide') : t('deploy.envShow')"
          @click="$emit('toggle')"
        >
          <HIcon :name="valueShown(row) ? 'eye-off' : 'eye'" size="sm" />
        </HButton>
      </template>
    </HInput>
    <HButton
      class="env__remove"
      variant="ghost"
      size="sm"
      data-env-remove
      :aria-label="t('deploy.envRemove', { n: position })"
      @click="$emit('remove')"
    >
      <HIcon name="close" size="sm" />
    </HButton>
  </div>
</template>

<style scoped>
.env {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 2fr) auto;
  align-items: start;
  gap: var(--space-3);
}

.env__remove {
  align-self: start;
}

/* La première rangée porte des libellés visibles : le bouton de retrait descend à hauteur des champs. */
.env--labelled .env__remove {
  margin-top: calc(var(--fs-small) * var(--lh-body) + var(--space-2));
}
</style>
