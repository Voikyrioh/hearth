<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// Fenêtre de formulaire (création de compte, mot de passe) : élément `<dialog>` natif ouvert par
// `showModal()` (piège à focus, reste de la page inerte, Échap), téléporté dans `body`. Le premier
// champ reçoit le focus à l'ouverture ; à la fermeture le focus revient à l'élément qui l'avait
// ouverte. Pendant l'envoi (`busy`) les champs sont figés (`<fieldset disabled>`) et la fenêtre ne
// se ferme pas. L'erreur (`error`) s'affiche DANS la fenêtre, qui reste ouverte. Le slot reçoit les
// champs ; le parent vide ses mots de passe après chaque envoi.
const props = withDefaults(
  defineProps<{
    open: boolean;
    title: string;
    submitLabel: string;
    canSubmit?: boolean;
    busy?: boolean;
    error?: string;
    destructive?: boolean;
    cancelLabel?: string;
  }>(),
  { canSubmit: true, busy: false, error: undefined, destructive: false, cancelLabel: undefined },
);

const emit = defineEmits<{ submit: []; cancel: [] }>();

const titleId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
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

function cancel() {
  if (!props.busy) emit("cancel");
}

function submit() {
  if (props.canSubmit && !props.busy) emit("submit");
}

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
      :aria-labelledby="titleId"
      @cancel.prevent="cancel"
      @close="onNativeClose"
    >
      <h2 :id="titleId" class="dialog__title">{{ title }}</h2>
      <form class="dialog__form" novalidate @submit.prevent="submit">
        <fieldset class="dialog__fields" :disabled="busy">
          <slot />
        </fieldset>
        <p v-if="error" class="dialog__error" role="alert">{{ error }}</p>
        <div class="dialog__actions">
          <HButton variant="secondary" :disabled="busy" @click="cancel">
            {{ cancelLabel ?? t("common.cancel") }}
          </HButton>
          <HButton
            type="submit"
            :variant="destructive ? 'danger' : 'primary'"
            :solid="destructive"
            :disabled="!canSubmit"
            :busy="busy"
          >
            {{ submitLabel }}
          </HButton>
        </div>
      </form>
    </dialog>
  </Teleport>
</template>

<style scoped>
.dialog {
  width: var(--form-dialog-width);
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

.dialog__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin-top: var(--space-4);
}

.dialog__fields {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.dialog__error {
  color: var(--crit);
}

.dialog__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
