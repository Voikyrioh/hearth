<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage, usernameProblemText } from "@/accounts/messages";
import HInput from "@/components/atoms/HInput.vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import PasswordRules from "@/components/molecules/PasswordRules.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { useAccountRules } from "@/composables/useAccountRules";
import { t } from "@/i18n";
import type { Role } from "@/link";

// Création d'un compte : identifiant, mot de passe, confirmation, rôle. La validation en direct est
// celle de l'agent (commande `check_account_input`, jamais une copie ici) ; l'agent reste l'arbitre
// à l'envoi. Les mots de passe sont vidés après CHAQUE envoi, réussi ou non ; l'identifiant et le
// rôle restent si l'agent a refusé. Rôle par défaut : Lecture seule (le moindre privilège).
const props = defineProps<{ open: boolean; serverId: string }>();
const emit = defineEmits<{ close: [] }>();

const username = ref("");
const password = ref("");
const confirmation = ref("");
const role = ref<Role>("readonly");
const usernameTouched = ref(false);
const passwordTouched = ref(false);
const confirmationTouched = ref(false);
const error = ref<string | undefined>();
const takenError = ref<string | undefined>();

const rules = useAccountRules(username, password);
const actions = useAccountActions(() => props.serverId);

const ROLES: Array<{ value: Role; label: string }> = [
  { value: "admin", label: t("accounts.roleAdmin") },
  { value: "readonly", label: t("accounts.roleReadonly") },
];

watch(
  () => props.open,
  (open) => {
    // Un mot de passe ne reste dans aucun champ une fois la fenêtre fermée.
    password.value = "";
    confirmation.value = "";
    if (!open) return;
    username.value = "";
    role.value = "readonly";
    usernameTouched.value = false;
    passwordTouched.value = false;
    confirmationTouched.value = false;
    error.value = undefined;
    takenError.value = undefined;
  },
);
watch(username, () => {
  usernameTouched.value = true;
  takenError.value = undefined;
});
watch(password, () => {
  passwordTouched.value = true;
});
watch(confirmation, () => {
  confirmationTouched.value = true;
});

const usernameError = computed(() => {
  if (takenError.value) return takenError.value;
  const problem = rules.check.value.username;
  return usernameTouched.value && problem ? usernameProblemText(problem) : undefined;
});
const mismatch = computed(() =>
  confirmationTouched.value && confirmation.value !== password.value
    ? t("accounts.mismatch")
    : undefined,
);
const canSubmit = computed(
  () =>
    rules.ready.value &&
    rules.check.value.username === null &&
    rules.check.value.password.length === 0 &&
    confirmation.value !== "" &&
    confirmation.value === password.value,
);

async function submit() {
  error.value = undefined;
  const report = await actions.create(username.value, password.value, role.value);
  // Un mot de passe ne reste jamais dans un champ après l'envoi, réussi ou non.
  password.value = "";
  confirmation.value = "";
  if (report.kind === "refused") {
    if (report.refusal.kind === "username_taken") takenError.value = refusalMessage(report.refusal);
    else error.value = refusalMessage(report.refusal);
    return;
  }
  // Fait, inconnu (déjà dit, jamais rejoué : la liste se relit au retour du lien) ou échec notifié.
  if (report.kind !== "failed") emit("close");
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="t('accounts.createTitle')"
    :submit-label="t('accounts.create')"
    :can-submit="canSubmit"
    :busy="actions.busy.value"
    :error="error"
    @submit="submit"
    @cancel="emit('close')"
  >
    <HInput
      v-model="username"
      :label="t('accounts.username')"
      :placeholder="t('accounts.usernamePlaceholder')"
      :error="usernameError"
      autocomplete="off"
    />
    <HPasswordInput
      v-model="password"
      :label="t('accounts.newPassword')"
      :placeholder="t('accounts.newPasswordPlaceholder')"
      autocomplete="new-password"
    />
    <PasswordRules :unmet="rules.check.value.password" :touched="passwordTouched" />
    <HPasswordInput
      v-model="confirmation"
      :label="t('accounts.confirmPassword')"
      :placeholder="t('accounts.confirmPasswordPlaceholder')"
      :error="mismatch"
      autocomplete="new-password"
    />
    <HSelect v-model="role" :label="t('accounts.role')" :options="ROLES" />
  </FormDialog>
</template>
