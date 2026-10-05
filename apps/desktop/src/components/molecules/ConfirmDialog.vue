<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// Confirmation d'une action : piège à focus, Échap = annuler, et le bouton par défaut
// (focus à l'ouverture) est toujours le sûr, « Annuler ». Le focus est rendu à
// l'élément qui avait ouvert la boîte.
const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    message: string;
    confirmLabel: string;
    cancelLabel?: string;
    destructive?: boolean;
  }>(),
  { cancelLabel: undefined, destructive: false },
);

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const titleId = useId();
const messageId = useId();
const dialog = ref<HTMLElement | null>(null);
const cancelButton = ref<{ $el: HTMLElement } | null>(null);
let opener: HTMLElement | null = null;

watch(
  () => props.open,
  async (open) => {
    if (open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      await nextTick();
      cancelButton.value?.$el.focus();
    } else {
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => opener?.focus());

function focusables(): HTMLElement[] {
  return [
    ...(dialog.value?.querySelectorAll<HTMLElement>("button, [href], input, select, textarea") ??
      []),
  ];
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.stopPropagation();
    emit("cancel");
    return;
  }
  if (event.key !== "Tab") return;
  const items = focusables();
  const first = items[0];
  const last = items[items.length - 1];
  if (!first || !last) return;
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <div v-if="open" class="scrim" @keydown="onKeydown">
    <div
      ref="dialog"
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      :aria-labelledby="titleId"
      :aria-describedby="messageId"
    >
      <h2 :id="titleId" class="dialog__title">{{ title }}</h2>
      <p :id="messageId" class="dialog__message">{{ message }}</p>
      <div class="dialog__actions">
        <HButton ref="cancelButton" variant="secondary" @click="emit('cancel')">
          {{ cancelLabel ?? t("common.cancel") }}
        </HButton>
        <HButton
          :variant="destructive ? 'danger' : 'primary'"
          :solid="destructive"
          @click="emit('confirm')"
        >
          {{ confirmLabel }}
        </HButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  z-index: var(--z-dialog);
  display: grid;
  place-items: center;
  background: var(--scrim);
}

.dialog {
  width: var(--dialog-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.dialog__title {
  font-size: var(--fs-h3);
  font-weight: 600;
}

.dialog__message {
  margin-top: var(--space-3);
  color: var(--tx2);
}

.dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
  margin-top: var(--space-5);
}
</style>
