<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// Notes de version : le texte brut annoncé par la release (jamais interprété comme du HTML),
// dans un `<dialog>` natif ouvert par `showModal()` (piège à focus et Échap gérés par le navigateur),
// téléporté dans `body`. Le focus revient à l'élément qui l'avait ouvert.
const props = defineProps<{ open: boolean; version: string; notes: string }>();
const emit = defineEmits<{ close: [] }>();

const titleId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
const closeButton = ref<{ $el: HTMLElement } | null>(null);
let opener: HTMLElement | null = null;

watch(
  () => props.open,
  (open) => {
    if (open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
  },
  { immediate: true, flush: "pre" },
);

watch(
  () => props.open,
  async (open) => {
    if (open) {
      await nextTick();
      dialog.value?.showModal();
      closeButton.value?.$el.querySelector("button")?.focus();
    } else {
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true, flush: "post" },
);

onBeforeUnmount(() => {
  opener?.focus();
  opener = null;
});

function onNativeClose() {
  if (props.open) emit("close");
}
</script>

<template>
  <Teleport to="body">
    <dialog
      v-if="open"
      ref="dialog"
      class="notes"
      role="dialog"
      :aria-labelledby="titleId"
      @cancel.prevent="emit('close')"
      @close="onNativeClose"
    >
      <h2 :id="titleId" class="notes__title">{{ t("updates.notes") }}</h2>
      <p class="notes__version">{{ t("updates.notesVersion", { version }) }}</p>
      <div class="notes__body" data-release-notes>{{ notes || t("updates.notesEmpty") }}</div>
      <div class="notes__actions">
        <HButton ref="closeButton" variant="secondary" @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.notes {
  width: var(--dialog-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  padding: var(--space-5);
  border: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  color: var(--tx);
  box-shadow: var(--card-edge);
}

.notes::backdrop {
  background: var(--scrim);
}

.notes__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.notes__version {
  margin-top: var(--space-1);
  color: var(--tx2);
  font-family: var(--font-mono);
}

.notes__body {
  max-height: var(--notes-max-height);
  margin-top: var(--space-4);
  overflow-y: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--tx);
}

.notes__actions {
  display: flex;
  justify-content: flex-end;
  margin-top: var(--space-5);
}
</style>
