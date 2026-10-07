<script setup lang="ts">
import { computed, ref, toRef, watch } from "vue";
import { refusalMessage } from "@/accounts/messages";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import PasswordRules from "@/components/molecules/PasswordRules.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { useAccountRules } from "@/composables/useAccountRules";
import { t } from "@/i18n";
import type { Account } from "@/link";
import { useSecurityStore } from "@/stores/security";

// Changement de mot de passe : le sien (`account` absent : ancien puis nouveau, ferme les AUTRES
// sessions, garde la courante, BR-ACCT-009) ou celui d'un autre compte (un administrateur le
// définit, ferme toutes les sessions de ce compte, BR-ACCT-008). `username` est celui du compte dont
// le mot de passe change : la règle « ne contient pas l'identifiant ». Les trois champs sont vidés
// après CHAQUE envoi, réussi ou non. Pour SON mot de passe, la case « Garder ce poste reconnu » (Q15,
// BR-CONN-019) est décochée par défaut : l'adresse d'où part la demande est alors oubliée comme les
// autres (le choix le plus strict). Absente de l'effet (désactivée, avec sa raison) quand l'agent est
// trop ancien pour la connaître ; et si le mode attaque est actif sur un poste sans clé enregistrée, un
// avertissement dit qu'oublier ce poste refuse la session tout de suite.
const props = defineProps<{
  open: boolean;
  serverId: string;
  username: string;
  /** Mode « mon mot de passe » (ancien puis nouveau) ; sinon `account` est le compte visé. */
  own: boolean;
  account?: Account | null;
}>();
const emit = defineEmits<{ close: [] }>();

const current = ref("");
const password = ref("");
const confirmation = ref("");
const passwordTouched = ref(false);
const confirmationTouched = ref(false);
const error = ref<string | undefined>();
const currentError = ref<string | undefined>();

const own = computed(() => props.own);
const keepAddress = ref(false);
const security = useSecurityStore();
const securityEntry = computed(() => security.of(props.serverId));
// L'agent connaît le choix s'il connaît la sécurité (HRT-24 les livre ensemble) ; tant que rien n'est
// lu, la case reste utilisable (l'agent ignore un champ qu'il ne connaît pas).
const keepSupported = computed(() => securityEntry.value?.status !== "unsupported");
const attackNoKey = computed(() => {
  const state = securityEntry.value?.state;
  return (
    state !== null && state !== undefined && state.attackMode.state !== "off" && !state.keyAtHand
  );
});
const rules = useAccountRules(toRef(props, "username"), password);
const actions = useAccountActions(() => props.serverId);

const title = computed(() =>
  own.value
    ? t("accounts.ownPasswordTitle")
    : t("accounts.setPasswordTitle", { username: props.username }),
);

watch(
  () => props.open,
  (open) => {
    // Un mot de passe ne reste dans aucun champ une fois la fenêtre fermée.
    current.value = "";
    password.value = "";
    confirmation.value = "";
    keepAddress.value = false;
    if (!open) return;
    passwordTouched.value = false;
    confirmationTouched.value = false;
    error.value = undefined;
    currentError.value = undefined;
  },
);
watch(password, () => {
  passwordTouched.value = true;
});
watch(confirmation, () => {
  confirmationTouched.value = true;
});
watch(current, (value) => {
  // L'effacement après l'envoi n'efface pas l'erreur qu'il vient de provoquer.
  if (value !== "") currentError.value = undefined;
});

const mismatch = computed(() =>
  confirmationTouched.value && confirmation.value !== password.value
    ? t("accounts.mismatch")
    : undefined,
);
const canSubmit = computed(
  () =>
    rules.ready.value &&
    (!own.value || current.value !== "") &&
    rules.check.value.password.length === 0 &&
    confirmation.value !== "" &&
    confirmation.value === password.value,
);

async function submit() {
  error.value = undefined;
  const report = props.account
    ? await actions.setPassword(props.account, password.value)
    : await actions.changeOwnPassword(
        current.value,
        password.value,
        own.value && keepAddress.value,
      );
  current.value = "";
  password.value = "";
  confirmation.value = "";
  if (report.kind === "refused") {
    if (report.refusal.kind === "wrong_password")
      currentError.value = refusalMessage(report.refusal);
    else error.value = refusalMessage(report.refusal);
    return;
  }
  if (report.kind !== "failed") emit("close");
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="title"
    :submit-label="t('accounts.changePassword')"
    :can-submit="canSubmit"
    :busy="actions.busy.value"
    :error="error"
    @submit="submit"
    @cancel="emit('close')"
  >
    <HPasswordInput
      v-if="own"
      v-model="current"
      :label="t('accounts.currentPassword')"
      :placeholder="t('accounts.currentPasswordPlaceholder')"
      :error="currentError"
      autocomplete="current-password"
    />
    <HPasswordInput
      v-model="password"
      :label="t('accounts.nextPassword')"
      :placeholder="t('accounts.nextPasswordPlaceholder')"
      autocomplete="new-password"
    />
    <PasswordRules :unmet="rules.check.value.password" :touched="passwordTouched" />
    <template v-if="own">
      <HCheckbox
        v-model="keepAddress"
        :label="t('accounts.keepAddress')"
        :disabled="!keepSupported"
        data-keep-address
      />
      <p class="password__help" data-keep-address-help>
        {{ t(keepSupported ? "accounts.keepAddressHelp" : "accounts.keepAddressOldAgent") }}
      </p>
      <p v-if="attackNoKey && !keepAddress" class="password__warn" role="status" data-keep-address-warn>
        {{ t("accounts.keepAddressAttackNoKey") }}
      </p>
    </template>
    <HPasswordInput
      v-model="confirmation"
      :label="t('accounts.confirmNextPassword')"
      :placeholder="t('accounts.confirmPasswordPlaceholder')"
      :error="mismatch"
      autocomplete="new-password"
    />
  </FormDialog>
</template>

<style scoped>
.password__help {
  color: var(--tx2);
}

.password__warn {
  color: var(--warn);
}
</style>
