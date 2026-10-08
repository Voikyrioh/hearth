<script setup lang="ts" generic="T extends string">
import { computed, nextTick, onBeforeUnmount, ref, useId } from "vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// Liste déroulante à choix multiple (comptes, types d'action, résultats du journal). Le bouton
// annonce la sélection ; la liste est un groupe de cases à cocher natives (clavier, lecteurs
// d'écran) ; Échap la ferme et rend le focus au bouton ; un clic ailleurs la ferme.
const props = defineProps<{
  modelValue: readonly T[];
  label: string;
  options: readonly { value: T; label: string }[];
}>();

const emit = defineEmits<{ "update:modelValue": [value: T[]] }>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
const button = ref<HTMLButtonElement | null>(null);
const listId = useId();
const labelId = useId();
const summaryId = useId();

const summary = computed(() => {
  if (props.modelValue.length === 0) return t("audit.anySelection");
  if (props.modelValue.length === 1) {
    return props.options.find((option) => option.value === props.modelValue[0])?.label ?? "";
  }
  return t("audit.selectionCount", { n: props.modelValue.length });
});

function toggle(value: T, on: boolean) {
  const next = props.modelValue.filter((item) => item !== value);
  if (on) next.push(value);
  // L'ordre des options est conservé.
  emit(
    "update:modelValue",
    props.options.map((option) => option.value).filter((item) => next.includes(item)),
  );
}

function onDocumentPointer(event: Event) {
  if (root.value && !root.value.contains(event.target as Node)) close(false);
}

function close(returnFocus: boolean) {
  open.value = false;
  document.removeEventListener("pointerdown", onDocumentPointer);
  if (returnFocus) void nextTick(() => button.value?.focus());
}

/** Le focus quitte le composant (Tab vers un autre champ) : la liste se referme. */
function onFocusOut(event: FocusEvent) {
  const next = event.relatedTarget as Node | null;
  if (open.value && next && root.value && !root.value.contains(next)) close(false);
}

function toggleOpen() {
  if (open.value) {
    close(false);
    return;
  }
  open.value = true;
  document.addEventListener("pointerdown", onDocumentPointer);
}

onBeforeUnmount(() => document.removeEventListener("pointerdown", onDocumentPointer));
</script>

<template>
  <div ref="root" class="multi" @keydown.esc.stop="close(true)" @focusout="onFocusOut">
    <span :id="labelId" class="multi__label">{{ label }}</span>
    <button
      ref="button"
      type="button"
      class="multi__button"
      :aria-labelledby="`${labelId} ${summaryId}`"
      aria-haspopup="true"
      :aria-expanded="open"
      :aria-controls="open ? listId : undefined"
      @click="toggleOpen"
    >
      <span :id="summaryId" class="multi__summary">{{ summary }}</span>
      <HIcon name="chevron-down" size="sm" />
    </button>
    <div v-if="open" :id="listId" class="multi__menu" role="group" :aria-label="label">
      <HCheckbox
        v-for="option in options"
        :key="option.value"
        :model-value="modelValue.includes(option.value)"
        :label="option.label"
        @update:model-value="(on: boolean) => toggle(option.value, on)"
      />
      <p v-if="options.length === 0" class="multi__none">{{ t("audit.none") }}</p>
    </div>
  </div>
</template>

<style scoped>
.multi {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.multi__label {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.multi__button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--field-pad-y) var(--field-pad-x);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  color: var(--tx);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.multi__button:focus-visible {
  border-color: var(--ac);
  /* FIX:01M4EPX90N9QNNPCHVNB6MCDDF (S3) : le halo est DESSINÉ DEDANS : il ne dépasse jamais l'axe gauche du contenu. */
  box-shadow: inset 0 0 0 var(--halo-width) var(--focus-halo);
  outline: none;
}

.multi__summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.multi__menu {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: var(--z-tooltip);
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  width: var(--audit-menu-width);
  max-height: var(--audit-menu-max);
  margin-top: var(--space-1);
  padding: var(--space-3) var(--space-4);
  overflow-y: auto;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
}

.multi__none {
  color: var(--tx3);
  font-size: var(--fs-small);
}
</style>
