<script setup lang="ts" generic="T extends string">
import { ref } from "vue";
import { arrowNav } from "@/composables/arrowNav";

// Onglets d'une même zone : `tablist`, flèches gauche et droite (Début, Fin) déplacent le FOCUS, Entrée ou Espace
// activent (activation manuelle : changer d'onglet peut coûter une lecture), un seul arrêt de tabulation (l'onglet
// choisi). Un onglet `disabled` reste atteignable aux flèches et au clavier, `aria-disabled`, et ne s'active
// jamais ; sa raison est écrite par la page qui l'accueille (`describedBy`). `idBase` : préfixe des `id` des
// onglets (`{idBase}-tab-{id}`) et des panneaux (`{idBase}-panel-{id}`), à poser sur les `role="tabpanel"`.
const props = defineProps<{
  modelValue: T;
  tabs: readonly { id: T; label: string; disabled?: boolean; describedBy?: string }[];
  label: string;
  idBase: string;
}>();

const emit = defineEmits<{ "update:modelValue": [id: T] }>();
const list = ref<HTMLElement | null>(null);

function pick(id: T, disabled: boolean | undefined) {
  if (!disabled) emit("update:modelValue", id);
}

function onKeydown(event: KeyboardEvent) {
  if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) {
    arrowNav(event, list.value, "[role=tab]");
  }
}
</script>

<template>
  <div ref="list" class="tabs" role="tablist" :aria-label="label" @keydown="onKeydown">
    <button
      v-for="tab in props.tabs"
      :id="`${idBase}-tab-${tab.id}`"
      :key="tab.id"
      type="button"
      role="tab"
      :class="['tabs__item', { 'tabs__item--on': tab.id === modelValue }]"
      :aria-selected="tab.id === modelValue"
      :aria-controls="`${idBase}-panel-${tab.id}`"
      :aria-disabled="tab.disabled ? 'true' : undefined"
      :aria-describedby="tab.disabled ? tab.describedBy : undefined"
      :tabindex="tab.id === modelValue ? 0 : -1"
      :data-tab="tab.id"
      @click="pick(tab.id, tab.disabled)"
    >
      {{ tab.label }}
    </button>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: var(--space-1);
  border-bottom: var(--border-width) solid var(--bd);
}

.tabs__item {
  min-height: var(--control-sm);
  padding: 0 var(--space-3);
  border: 0;
  border-bottom: var(--space-half) solid transparent;
  background: transparent;
  color: var(--tx2);
  cursor: pointer;
  white-space: nowrap;
}

.tabs__item:hover {
  color: var(--tx);
}

.tabs__item--on {
  border-bottom-color: var(--ac);
  color: var(--tx);
  font-weight: var(--fw-semibold);
}

.tabs__item[aria-disabled="true"] {
  color: var(--tx3);
  cursor: default;
}
</style>
