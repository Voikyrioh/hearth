<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { useDeviceActions } from "@/composables/useDeviceActions";
import { removalRefusalMessage } from "@/devices/messages";
import { t } from "@/i18n";
import type { TrustedDevice } from "@/link";

// Retrait d'un poste de confiance : un acte d'administration (Q16), donc le mot de passe actuel est
// demandé (la preuve de la clé de ce PC, elle, est faite par la coquille, sans geste de l'utilisateur).
// Le champ est vidé après CHAQUE envoi, réussi ou non, et à chaque ouverture : un mot de passe ne
// reste dans aucun champ ni aucun état une fois la fenêtre fermée. Un refus s'affiche DANS la
// fenêtre (mot de passe faux : sous le champ) ; le lien coupé avant la réponse ferme la fenêtre, le
// résultat inconnu est dit et l'action n'est jamais rejouée (BR-RESIL-009).
const props = defineProps<{ open: boolean; serverId: string; device: TrustedDevice | null }>();
const emit = defineEmits<{ close: [] }>();

const password = ref("");
const error = ref<string | undefined>();
const passwordError = ref<string | undefined>();
const actions = useDeviceActions(() => props.serverId);

watch(
  () => props.open,
  () => {
    password.value = "";
    error.value = undefined;
    passwordError.value = undefined;
  },
);
watch(password, (value) => {
  // L'effacement après l'envoi n'efface pas l'erreur qu'il vient de provoquer.
  if (value !== "") passwordError.value = undefined;
});

const title = computed(() => t("devices.removeTitle", { name: props.device?.name ?? "" }));

async function submit() {
  const device = props.device;
  if (!device) return;
  error.value = undefined;
  const report = await actions.remove(device, password.value);
  password.value = "";
  if (report.kind === "refused") {
    if (report.refusal.kind === "wrong_password") {
      passwordError.value = removalRefusalMessage(report.refusal);
    } else error.value = removalRefusalMessage(report.refusal);
    return;
  }
  if (report.kind !== "failed") emit("close");
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="title"
    :submit-label="t('devices.remove')"
    :can-submit="password !== ''"
    :busy="actions.busy.value"
    :error="error"
    destructive
    @submit="submit"
    @cancel="emit('close')"
  >
    <p class="remove__message">{{ t("devices.removeMessage") }}</p>
    <HPasswordInput
      v-model="password"
      :label="t('devices.password')"
      :placeholder="t('devices.passwordPlaceholder')"
      :error="passwordError"
      autocomplete="current-password"
    />
    <!-- FIX:01M4ECZJKG6MQ2C65TZP25WSPD -->
    <details class="remove__more">
      <summary>{{ t("devices.removeAdviceTitle") }}</summary>
      <p class="remove__help" data-remove-advice>{{ t("devices.removeAdvice") }}</p>
    </details>
  </FormDialog>
</template>

<style scoped>
.remove__message {
  color: var(--tx);
}

.remove__help {
  color: var(--tx2);
}

.remove__more summary {
  cursor: pointer;
  color: var(--ac);
}
</style>
