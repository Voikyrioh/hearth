<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage, usernameProblemText } from "@/accounts/messages";
import HInput from "@/components/atoms/HInput.vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import PasswordRules from "@/components/molecules/PasswordRules.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { useAccountRules } from "@/composables/useAccountRules";
import { t } from "@/i18n";
import type { Role } from "@/link";

// Création d'un compte : identifiant, mot de passe, confirmation, rôle. La validation en direct est
// celle de l'agent (commande `check_account_input`, jamais une copie ici) ; l'agent reste l'arbitre
// à l'envoi. Les mots de passe du nouveau compte sont vidés après un envoi non refusé ; sur un refus tout
// reste (seule la confirmation est à refaire, C18) ; l'identifiant et le rôle restent. Rôle par défaut : Lecture seule (le moindre privilège).
const props = defineProps<{ open: boolean; serverId: string }>();
const emit = defineEmits<{ close: [] }>();

const username = ref("");
const password = ref("");
const confirmation = ref("");
const role = ref<Role>("readonly");
const usernameTouched = ref(false);
const passwordTouched = ref(false);
const confirmationTouched = ref(false);
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

// La confirmation (mot de passe, preuve de la clé) est portée par la fenêtre commune des actes. Le mot de
// passe du NOUVEAU compte est vidé après chaque envoi, sauf si le délai de 5 minutes s'est fermé entre-temps
// (rien n'a été fait : la fenêtre redemande le mot de passe de confirmation sans perdre la saisie).
async function perform(adminPassword: string | null) {
  takenError.value = undefined;
  const report = await actions.create(username.value, password.value, role.value, adminPassword);
  // FIX:01M4D0RJB17YE0F26QFXGJ2WEV (C18) : un refus (mot de passe de confirmation faux, attente, délai fermé,
  // identifiant pris…) garde la saisie du nouveau compte ; la fenêtre ne vide que le mot de passe de
  // confirmation. Les mots de passe du nouveau compte ne sont vidés que si l'acte a été envoyé sans
  // refus (fait, résultat inconnu, échec).
  if (report.kind !== "refused") {
    password.value = "";
    confirmation.value = "";
  }
  if (report.kind === "refused" && report.refusal.kind === "username_taken") {
    takenError.value = refusalMessage(report.refusal);
  }
  return report;
}

function refusalText(refusal: { kind: string }): string | undefined {
  if (refusal.kind === "username_taken") return undefined;
  return refusalMessage(refusal as never);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="account_create"
    :role="role"
    :title="t('accounts.createTitle')"
    :submit-label="t('accounts.create')"
    :can-submit="canSubmit"
    :perform="perform"
    :refusal-text="refusalText"
    @close="emit('close')"
  >
    <!-- FIX:01M4D0RJJNZK7NGDMSFBE18Q29 (C19) : chaque mot de passe dit à qui il est, l'exemple d'identifiant est neutre. -->
    <HInput
      v-model="username"
      :label="t('accounts.username')"
      :placeholder="t('accounts.newUsernamePlaceholder')"
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
  </AdminActDialog>
</template>
