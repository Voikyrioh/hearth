<script setup lang="ts">
import { computed, useId } from "vue";
import { safeText } from "@/audit/text";
import HIcon from "@/components/atoms/HIcon.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import { t } from "@/i18n";
import type { FicheDetail } from "@/link";

// Le choix de la variante d'une fiche (HRT-102, BR-FICHE-060) : en tête de l'étape « Réponses », avant le nom du
// service. Jusqu'à quatre variantes, des cartes exclusives (un groupe de boutons radio natifs : clavier et lecteurs
// d'écran), chacune avec le libellé et la phrase d'aide ÉCRITS PAR LA FICHE ; au-delà, une liste avec l'aide de la
// variante choisie. La variante proposée est présélectionnée. Elle ne se change plus après l'installation : la
// mention le dit sous le choix. Une fiche à variante unique n'affiche pas cette carte.
const props = defineProps<{
  variants: FicheDetail["variants"];
  modelValue: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const name = useId();
const cards = computed(() => props.variants.length <= 4);
const options = computed(() =>
  props.variants.map((variant) => ({ value: variant.id, label: safeText(variant.label) })),
);
const chosen = computed(() => props.variants.find((variant) => variant.id === props.modelValue));
</script>

<template>
  <div class="variant" data-variant-choice>
    <fieldset v-if="cards" class="variant__set">
      <legend class="variant__legend">{{ t("fiche.variantLabel") }}</legend>
      <label
        v-for="variant in variants"
        :key="variant.id"
        :class="['variant__card', { 'variant__card--on': variant.id === modelValue }]"
        data-variant-card
      >
        <input
          class="variant__radio"
          type="radio"
          :name="name"
          :value="variant.id"
          :checked="variant.id === modelValue"
          @change="emit('update:modelValue', variant.id)"
        />
        <span class="variant__text">
          <span class="variant__name">{{ safeText(variant.label) }}</span>
          <span class="variant__help">{{ safeText(variant.help) }}</span>
        </span>
        <HIcon v-if="variant.id === modelValue" name="check" size="sm" />
      </label>
    </fieldset>
    <template v-else>
      <HSelect
        :model-value="modelValue"
        :label="t('fiche.variantLabel')"
        :options="options"
        @update:model-value="emit('update:modelValue', $event)"
      />
      <p v-if="chosen" class="variant__help">{{ safeText(chosen.help) }}</p>
    </template>
    <p class="variant__frozen" data-variant-frozen>
      <HIcon name="lock" size="sm" />
      {{ t("fiche.variantFrozen") }}
    </p>
  </div>
</template>

<style scoped>
.variant {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.variant__set {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
  gap: var(--space-3);
  min-width: 0;
  padding: 0;
  margin: 0;
  border: 0;
}

.variant__legend {
  margin-bottom: var(--space-2);
  padding: 0;
  font-weight: var(--fw-medium);
}

.variant__card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  cursor: pointer;
}

.variant__card--on {
  border-color: var(--ac);
}

.variant__card:focus-within {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.variant__radio {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.variant__text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.variant__name {
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.variant__help {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.variant__frozen {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--warn);
  font-size: var(--fs-small);
}
</style>
