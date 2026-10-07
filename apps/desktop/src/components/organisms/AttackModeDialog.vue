<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { useAttackModeActions } from "@/composables/useAttackModeActions";
import { t } from "@/i18n";
import { attackModeRefusalMessage } from "@/security/messages";

// Activation ou désactivation du mode attaque : un acte d'administration (Q14 point 3, Q16). La
// confirmation demande le mot de passe actuel ; la preuve de la clé de ce PC, elle, est faite par la
// coquille, sans geste de l'utilisateur. Le bouton de confirmation n'est pas destructeur : activer
// n'est pas une destruction. Le champ est vidé après CHAQUE envoi, réussi ou non, et à chaque
// ouverture : un mot de passe ne reste dans aucun champ ni aucun état une fois la fenêtre fermée. Un
// refus s'affiche DANS la fenêtre (mot de passe faux : sous le champ) ; le lien coupé avant la réponse
// ferme la fenêtre, le résultat inconnu est dit et l'action n'est jamais rejouée (BR-RESIL-009).
const props = defineProps<{ open: boolean; serverId: string; active: boolean }>();
const emit = defineEmits<{ close: [] }>();

const password = ref("");
const error = ref<string | undefined>();
const passwordError = ref<string | undefined>();
const actions = useAttackModeActions(() => props.serverId);

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

const title = computed(() =>
  t(props.active ? "security.confirmOnTitle" : "security.confirmOffTitle"),
);
const message = computed(() =>
  t(props.active ? "security.confirmOnMessage" : "security.confirmOffMessage"),
);
const submitLabel = computed(() => t(props.active ? "security.activate" : "security.deactivate"));

async function submit() {
  error.value = undefined;
  const report = await actions.change(props.active, password.value);
  password.value = "";
  if (report.kind === "refused") {
    if (report.refusal.kind === "wrong_password") {
      passwordError.value = attackModeRefusalMessage(report.refusal);
    } else error.value = attackModeRefusalMessage(report.refusal);
    return;
  }
  if (report.kind !== "failed") emit("close");
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="title"
    :submit-label="submitLabel"
    :can-submit="password !== ''"
    :busy="actions.busy.value"
    :error="error"
    @submit="submit"
    @cancel="emit('close')"
  >
    <p class="attack__message">{{ message }}</p>
    <p class="attack__help">{{ t("security.passwordHelp") }}</p>
    <HPasswordInput
      v-model="password"
      :label="t('security.password')"
      :placeholder="t('security.passwordPlaceholder')"
      :error="passwordError"
      autocomplete="current-password"
    />
  </FormDialog>
</template>

<style scoped>
.attack__message {
  color: var(--tx);
}

.attack__help {
  color: var(--tx2);
}
</style>
