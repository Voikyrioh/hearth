<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HInput from "@/components/atoms/HInput.vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import { t } from "@/i18n";

// Formulaire de connexion à un serveur : identifiant, mot de passe, « Se souvenir de moi sur ce
// PC » (cochée par défaut, BR-CONN-004). Sert au 3e temps de l'ajout et à la reconnexion d'un
// serveur enregistré. Pendant la connexion les champs sont figés ; « trop de tentatives » remplace
// le bouton par le compte à rebours (BR-CONN-006) ; l'erreur de connexion ne dit pas lequel des
// deux champs est faux (BR-CONN-013).
const props = withDefaults(
  defineProps<{
    username?: string;
    busy?: boolean;
    /** Erreur de la dernière tentative (texte), affichée sous le mot de passe. */
    error?: string | null;
    /** Secondes d'attente restantes avant une nouvelle tentative, `0` si aucune. */
    lockedSeconds?: number;
    submitLabel?: string;
    /** Bouton « Précédent » (ou autre) à gauche du bouton principal. */
    backLabel?: string;
    remember?: boolean;
  }>(),
  {
    username: "",
    busy: false,
    error: null,
    lockedSeconds: 0,
    submitLabel: undefined,
    backLabel: undefined,
    remember: true,
  },
);

const emit = defineEmits<{
  submit: [login: { username: string; password: string; remember: boolean }];
  back: [];
}>();

const user = ref(props.username);
const password = ref("");
const keep = ref(props.remember);
const tried = ref(false);
watch(
  () => props.username,
  (value) => {
    if (user.value === "") user.value = value;
  },
);

const userError = computed(() =>
  tried.value && user.value.trim() === "" ? t("validation.usernameRequired") : undefined,
);
const passwordError = computed(() => {
  if (tried.value && password.value === "") return t("validation.passwordRequired");
  return props.error ?? undefined;
});

function submit() {
  tried.value = true;
  if (props.busy || props.lockedSeconds > 0) return;
  if (user.value.trim() === "" || password.value === "") return;
  emit("submit", { username: user.value.trim(), password: password.value, remember: keep.value });
}

const root = ref<HTMLFormElement | null>(null);

defineExpose({
  /** Le curseur va dans le premier champ à remplir (identifiant s'il est vide, sinon mot de passe). */
  focusFirstEmpty: () => {
    const fields = root.value?.querySelectorAll<HTMLInputElement>("input:not([type=checkbox])");
    for (const input of fields ?? []) {
      if (input.value === "") {
        input.focus();
        return;
      }
    }
  },
  /** Vide le mot de passe (après un refus, il n'est jamais gardé à l'écran). */
  clearPassword: () => {
    password.value = "";
    tried.value = false;
  },
});
</script>

<template>
  <form ref="root" class="login" novalidate @submit.prevent="submit">
    <HInput
      v-model="user"
      :label="t('connect.username')"
      autocomplete="username"
      :disabled="busy"
      :error="userError"
    />
    <HPasswordInput
      v-model="password"
      :label="t('connect.password')"
      autocomplete="current-password"
      :disabled="busy"
      :error="passwordError"
    />
    <HCheckbox v-model="keep" :label="t('connect.remember')" :disabled="busy" />
    <p v-if="busy" class="login__status" role="status">{{ t("connect.connecting") }}</p>
    <p v-else-if="lockedSeconds > 0" class="login__locked" role="alert">
      {{ t("failure.tooManyAttempts", { n: lockedSeconds }) }}
    </p>
    <div class="login__actions">
      <HButton v-if="backLabel" variant="secondary" :disabled="busy" @click="emit('back')">
        {{ backLabel }}
      </HButton>
      <HButton v-if="lockedSeconds <= 0" type="submit" :busy="busy">
        {{ submitLabel ?? t("connect.login") }}
      </HButton>
    </div>
  </form>
</template>

<style scoped>
.login {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.login__status {
  color: var(--tx2);
}

.login__locked {
  color: var(--crit);
}

.login__actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
