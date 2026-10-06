<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
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
    /** Action en cours : les deux boutons attendent, la fenêtre ne se ferme pas. */
    busy?: boolean;
    /** Échec de l'action : montré DANS la fenêtre, qui reste ouverte (annuler ou réessayer). */
    error?: string;
  }>(),
  { cancelLabel: undefined, destructive: false, busy: false, error: undefined },
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

// Démonté pendant qu'il est ouvert (changement de route) : le focus revient quand même.
onBeforeUnmount(() => {
  opener?.focus();
  opener = null;
});

// Fermeture native (sans passer par nos boutons ni Échap) : on le dit au parent pour que
// `open` reste cohérent avec ce que l'écran montre.
function onNativeClose() {
  if (props.open) emit("cancel");
}
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
      @cancel.prevent="busy || emit('cancel')"
      @close="onNativeClose"
    >
      <h2 :id="titleId" class="dialog__title">{{ title }}</h2>
      <p :id="messageId" class="dialog__message">{{ message }}</p>
      <p v-if="error" class="dialog__error" role="alert">{{ error }}</p>
      <div class="dialog__actions">
        <HButton ref="cancelButton" variant="secondary" :disabled="busy" @click="emit('cancel')">
          {{ cancelLabel ?? t("common.cancel") }}
        </HButton>
        <HButton
          :variant="destructive ? 'danger' : 'primary'"
          :solid="destructive"
          :busy="busy"
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

.dialog__error {
  margin-top: var(--space-3);
  color: var(--crit);
}

.dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
  margin-top: var(--space-5);
}
</style>
