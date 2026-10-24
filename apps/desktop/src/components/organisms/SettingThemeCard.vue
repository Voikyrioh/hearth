<script setup lang="ts">
import { ref, useId, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// Une carte de thème repliable (HRT-109, BR-FICHE-041, design 16.2) : un `details` natif (clavier et lecteur d'écran
// sans code), le titre du thème en `h2`, le compteur « {n} réglages, {m} modifié(s) » et le lien « Remettre ce thème aux
// valeurs proposées ». Ouverte par défaut quand le thème contient un réglage essentiel ou modifié ; une recherche ouvre
// les thèmes qui contiennent un résultat (`forceOpen`). Le contenu est le slot : les lignes de réglage.
const props = defineProps<{
  /** Le titre du thème, tel que la fiche le dit. */
  label: string;
  /** Combien de réglages le thème compte en tout, et combien sont modifiés. */
  total: number;
  modified: number;
  /** Ouverte au départ. */
  open: boolean;
  /** Une recherche est en cours : le thème s'ouvre. */
  forceOpen?: boolean;
  /** La remise du thème est proposée (au moins un réglage à remettre). */
  canReset?: boolean;
  disabled?: boolean;
}>();

const emit = defineEmits<{ reset: [] }>();

const titleId = useId();
const isOpen = ref(props.open);
watch(
  () => props.forceOpen,
  (force) => {
    if (force) isOpen.value = true;
  },
  { immediate: true },
);
function onToggle(event: Event) {
  isOpen.value = (event.target as HTMLDetailsElement).open;
}
</script>

<template>
  <details
    class="theme"
    :open="isOpen"
    :aria-labelledby="titleId"
    data-setting-theme
    @toggle="onToggle"
  >
    <summary class="theme__summary">
      <h2 :id="titleId" class="theme__title">{{ safeText(label) }}</h2>
      <span class="theme__count" data-theme-count>
        {{ t("gameSettings.themeCount", { n: total, m: modified }) }}
      </span>
    </summary>
    <div class="theme__body">
      <div v-if="canReset" class="theme__reset">
        <HButton
          variant="ghost"
          size="sm"
          :disabled="disabled"
          data-theme-reset
          @click="emit('reset')"
        >
          {{ t("gameSettings.themeReset") }}
        </HButton>
      </div>
      <slot />
    </div>
  </details>
</template>

<style scoped>
.theme {
  min-width: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.theme__summary {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2) var(--space-4);
  padding: var(--space-4) var(--space-5);
  cursor: pointer;
}

.theme__summary:focus-visible {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.theme__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.theme__count {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.theme__body {
  display: flex;
  flex-direction: column;
  min-width: 0;
  padding: 0 var(--space-5) var(--space-4);
}

.theme__reset {
  display: flex;
  justify-content: flex-start;
  padding-bottom: var(--space-2);
}
</style>
