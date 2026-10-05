<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import FingerprintBlock from "@/components/molecules/FingerprintBlock.vue";
import { t } from "@/i18n";
import type { FingerprintChange } from "@/link";

// Alerte d'identité changée (BR-CONN-003) : le seul écran bloquant de l'application. Elle montre
// les deux empreintes côte à côte ; « Ne pas se connecter » est l'action par défaut (focus à
// l'ouverture, Échap), « Accepter la nouvelle empreinte » est un bouton contour, jamais plein.
// Élément `<dialog>` natif modal : le reste de la page est inerte.
const props = defineProps<{ change: FingerprintChange; serverName: string }>();

const emit = defineEmits<{ refuse: []; accept: [] }>();

const titleId = useId();
const textId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
const refuseButton = ref<{ $el: HTMLElement } | null>(null);
const accepting = ref(false);

watch(
  () => props.change.presentedHex,
  async () => {
    await nextTick();
    if (dialog.value && !dialog.value.open) dialog.value.showModal();
    refuseButton.value?.$el.querySelector("button")?.focus();
  },
  { immediate: true },
);

onBeforeUnmount(() => dialog.value?.close());

function accept() {
  accepting.value = true;
  emit("accept");
}
</script>

<template>
  <Teleport to="body">
    <dialog
      ref="dialog"
      class="alert"
      role="alertdialog"
      :aria-labelledby="titleId"
      :aria-describedby="textId"
      data-fingerprint-alert
      @cancel.prevent="emit('refuse')"
    >
      <h2 :id="titleId" class="alert__title">
        {{ t("fingerprintAlert.title") }} : {{ serverName }}
      </h2>
      <p :id="textId" class="alert__text">{{ t("fingerprintAlert.text") }}</p>
      <div class="alert__prints">
        <FingerprintBlock :value="change.expected" :label="t('fingerprintAlert.stored')" />
        <FingerprintBlock
          :value="change.presented"
          :label="t('fingerprintAlert.received')"
          tone="crit"
        />
      </div>
      <div class="alert__actions">
        <HButton ref="refuseButton" variant="secondary" @click="emit('refuse')">
          {{ t("fingerprintAlert.refuse") }}
        </HButton>
        <HButton variant="danger" :busy="accepting" @click="accept">
          {{ t("fingerprintAlert.accept") }}
        </HButton>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.alert {
  width: var(--panel-max);
  max-width: calc(100vw - var(--page-pad) * 2);
  padding: var(--space-5);
  border: var(--border-width) solid var(--crit);
  border-radius: var(--radius-card);
  background: var(--crit-tint);
  color: var(--tx);
}

.alert::backdrop {
  background: var(--scrim);
}

.alert__title {
  color: var(--crit);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.alert__text {
  margin-top: var(--space-3);
  color: var(--tx2);
}

.alert__prints {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-4);
  margin-top: var(--space-4);
}

.alert__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
  margin-top: var(--space-5);
}
</style>
