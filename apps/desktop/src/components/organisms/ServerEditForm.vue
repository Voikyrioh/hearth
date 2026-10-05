<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import ColorSwatches from "@/components/molecules/ColorSwatches.vue";
import FingerprintBlock from "@/components/molecules/FingerprintBlock.vue";
import { useEditServer } from "@/composables/useEditServer";
import { t } from "@/i18n";
import type { ServerInfo } from "@/link";

// Modification d'un serveur enregistré : nom, couleur, adresse. La logique (une autre adresse exige
// de relire et confirmer l'empreinte, BR-CONN-009) est dans `useEditServer` ; ici, l'affichage.
const props = defineProps<{ server: ServerInfo }>();
const emit = defineEmits<{ done: [server: ServerInfo]; cancel: [] }>();

const edit = useEditServer(() => props.server);

async function submit() {
  const saved = await edit.submit();
  if (saved) emit("done", saved);
}

async function confirm() {
  const saved = await edit.confirm();
  if (saved) emit("done", saved);
}
</script>

<template>
  <section class="edit" :aria-label="t('connect.editTitle')">
    <form v-if="edit.step.value === 'form'" class="edit__form" novalidate @submit.prevent="submit">
      <h2 class="edit__title">{{ t("connect.editTitle") }}</h2>
      <HInput
        v-model="edit.name.value"
        :label="t('connect.name')"
        :disabled="edit.busy.value"
        :error="edit.messages.value.name"
      />
      <div class="edit__row">
        <div class="edit__host">
          <HInput
            v-model="edit.host.value"
            :label="t('connect.host')"
            :disabled="edit.busy.value"
            :error="edit.messages.value.host"
          />
        </div>
        <div class="edit__port">
          <HInput
            v-model="edit.port.value"
            :label="t('connect.port')"
            :placeholder="t('connect.portPlaceholder')"
            :disabled="edit.busy.value"
            :error="edit.messages.value.port"
            mono
          />
        </div>
      </div>
      <ColorSwatches v-model="edit.color.value" :label="t('connect.color')" :disabled="edit.busy.value" />
      <p v-if="edit.moved.value" class="edit__info" role="status">{{ t("connect.moved") }}</p>
      <p v-if="edit.failure.value" class="edit__error" role="alert">{{ edit.failure.value }}</p>
      <div class="edit__actions">
        <HButton variant="secondary" :disabled="edit.busy.value" @click="emit('cancel')">
          {{ t("connect.cancel") }}
        </HButton>
        <HButton type="submit" :busy="edit.busy.value" :disabled="!edit.valid.value">
          {{ edit.moved.value ? t("connect.next") : t("connect.save") }}
        </HButton>
      </div>
    </form>

    <div v-else-if="edit.probe.value" class="edit__form">
      <h2 class="edit__title">{{ t("connect.verifyTitle") }}</h2>
      <FingerprintBlock :value="edit.probe.value.display" :label="t('connect.fingerprintLabel')" />
      <p class="edit__info">{{ t("connect.fingerprintHelp") }}</p>
      <div class="edit__actions">
        <HButton variant="secondary" :disabled="edit.busy.value" @click="edit.refuse()">
          {{ t("connect.refuseFingerprint") }}
        </HButton>
        <HButton :busy="edit.busy.value" @click="confirm">
          {{ t("connect.confirmFingerprint") }}
        </HButton>
      </div>
    </div>
  </section>
</template>

<style scoped>
.edit {
  padding: var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card-2);
}

.edit__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.edit__title {
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.edit__row {
  display: flex;
  gap: var(--space-3);
}

.edit__host {
  flex: 3;
}

.edit__port {
  flex: 1;
}

.edit__info {
  color: var(--tx2);
}

.edit__error {
  color: var(--crit);
}

.edit__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
