<script setup lang="ts">
import { computed, ref } from "vue";
import { t } from "@/i18n";
import { MAX_QUERY } from "@/logs/search";
import HButton from "../atoms/HButton.vue";
import HIcon from "../atoms/HIcon.vue";

// La recherche dans les lignes chargées (HRT-67) : un champ, le compteur « 3 sur 12 », résultat précédent et suivant.
// Entrée : suivant ; Maj+Entrée : précédent ; Échap : efface. La requête est du texte, jamais un motif. Le compteur
// n'est pas une zone live : il se lit à la demande, et son changement n'est pas annoncé à chaque ligne qui arrive.
const props = defineProps<{
  modelValue: string;
  /** Numéro (à partir de 1) du résultat courant, 0 : aucun. */
  current: number;
  total: number;
  /** Il y a plus de résultats que `total`. */
  more: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  next: [];
  previous: [];
}>();

const input = ref<HTMLInputElement | null>(null);
const searching = computed(() => props.modelValue.trim() !== "");
const count = computed(() => {
  if (!searching.value) return "";
  if (props.total === 0) return t("logs.searchNone");
  return t(props.more ? "logs.searchCountMore" : "logs.searchCount", {
    current: props.current,
    total: props.total,
  });
});

function onKey(event: KeyboardEvent) {
  if (event.key === "Enter") {
    event.preventDefault();
    if (event.shiftKey) emit("previous");
    else emit("next");
  } else if (event.key === "Escape" && searching.value) {
    event.stopPropagation();
    emit("update:modelValue", "");
  }
}

function clear() {
  emit("update:modelValue", "");
  input.value?.focus();
}

defineExpose({ focus: () => input.value?.focus() });
</script>

<template>
  <div class="search" data-log-search role="search">
    <label class="search__field">
      <HIcon name="search" size="sm" />
      <span class="sr-only">{{ t("logs.searchLabel") }}</span>
      <input
        ref="input"
        class="search__input"
        type="search"
        autocomplete="off"
        spellcheck="false"
        :maxlength="MAX_QUERY"
        :value="modelValue"
        :placeholder="t('logs.searchPlaceholder')"
        data-log-search-input
        @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
        @keydown="onKey"
      />
    </label>
    <span v-if="searching" class="search__count" data-log-search-count>{{ count }}</span>
    <template v-if="searching">
      <HButton
        variant="ghost"
        size="sm"
        :disabled="total === 0"
        :aria-label="t('logs.searchPrevious')"
        data-log-search-prev
        @click="emit('previous')"
      >
        <HIcon name="arrow-up" size="sm" />
      </HButton>
      <HButton
        variant="ghost"
        size="sm"
        :disabled="total === 0"
        :aria-label="t('logs.searchNext')"
        data-log-search-next
        @click="emit('next')"
      >
        <HIcon name="arrow-down" size="sm" />
      </HButton>
      <HButton
        variant="ghost"
        size="sm"
        :aria-label="t('logs.searchClear')"
        data-log-search-clear
        @click="clear"
      >
        <HIcon name="close" size="sm" />
      </HButton>
    </template>
  </div>
</template>

<style scoped>
.search {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
}

.search__field {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  height: var(--control-sm);
  padding: 0 var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  color: var(--tx-secondary);
}

.search__field:focus-within {
  border-color: var(--ac);
  box-shadow: 0 0 0 var(--space-half) var(--focus-halo);
}

.search__input {
  width: 100%;
  min-width: 0;
  border: 0;
  outline: 0;
  background: transparent;
  color: var(--tx);
  font: inherit;
}

.search__input::placeholder {
  color: var(--tx3);
}

.search__count {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
</style>
