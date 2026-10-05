<script setup lang="ts">
import { nextTick, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// Confirmation d'une action : élément `<dialog>` natif ouvert par `showModal()` (le navigateur
// fait le piège à focus, rend le reste de la page inerte et gère Échap), téléporté dans `body`.
// Le bouton par défaut (focus à l'ouverture) est toujours le sûr, « Annuler ». À la fermeture
// le focus revient à l'élément qui avait ouvert la boîte.
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
const dialog = ref<HTMLDialogElement | null>(null);
const cancelButton = ref<{ $el: HTMLElement } | null>(null);
let opener: HTMLElement | null = null;

// Avant l'affichage : l'élément qui a le focus est celui qui a ouvert la boîte.
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
      cancelButton.value?.$el.querySelector("button")?.focus();
    } else {
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true, flush: "post" },
);
</script>

<template>
  <Teleport to="body">
    <dialog
      v-if="open"
      ref="dialog"
      class="dialog"
      role="alertdialog"
      :aria-labelledby="titleId"
      :aria-describedby="messageId"
      @cancel.prevent="emit('cancel')"
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
    </dialog>
  </Teleport>
</template>

<style scoped>
.dialog {
  width: var(--dialog-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  padding: var(--space-5);
  border: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  color: var(--tx);
  box-shadow: var(--card-edge);
}

.dialog::backdrop {
  background: var(--scrim);
}

.dialog__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
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
