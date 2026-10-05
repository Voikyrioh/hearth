<script setup lang="ts">
import { computed, ref } from "vue";
import LoginForm from "@/components/organisms/LoginForm.vue";
import { useCountdown } from "@/composables/useCountdown";
import { t } from "@/i18n";
import { failureMessage, failureOf, getLinkBridge, type Reason, type ServerInfo } from "@/link";

// Serveur enregistré sans session : session expirée, déconnexion volontaire, première connexion
// interrompue, ou mot de passe mémorisé devenu invalide. Le formulaire de connexion remplace la
// page, identifiant prérempli. Mot de passe mémorisé refusé : aucun message bloquant, juste le
// formulaire (BR-CONN-017). Accès révoqué : rien à saisir, on dit à qui s'adresser.
const props = defineProps<{ server: ServerInfo; reason: Reason | null; revoked?: boolean }>();

const countdown = useCountdown();
const busy = ref(false);
const error = ref<string | null>(null);
const form = ref<InstanceType<typeof LoginForm> | null>(null);

const notice = computed(() => {
  if (props.revoked) return t("connect.accessRevoked");
  return props.reason === "expired" ? t("connect.sessionExpired") : null;
});

async function submit(entry: { username: string; password: string; remember: boolean }) {
  if (busy.value || countdown.active.value) return;
  error.value = null;
  busy.value = true;
  try {
    await getLinkBridge().login(props.server.id, entry.username, entry.password, entry.remember);
  } catch (failure) {
    const reason = failureOf(failure);
    if (reason?.kind === "too_many_attempts") countdown.start(reason.retry_after_s);
    else error.value = reason ? failureMessage(reason) : t("failure.generic");
    form.value?.clearPassword();
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="reconnect" :aria-label="t('connect.reconnectTitle', { name: server.name })">
    <h2 class="reconnect__title">{{ t("connect.reconnectTitle", { name: server.name }) }}</h2>
    <p v-if="notice" class="reconnect__notice" role="status">{{ notice }}</p>
    <LoginForm
      v-if="!revoked"
      ref="form"
      :username="server.username"
      :busy="busy"
      :error="error"
      :locked-seconds="countdown.remaining.value"
      :remember="server.remember || reason !== 'stored_password_refused'"
      @submit="submit"
    />
  </section>
</template>

<style scoped>
.reconnect {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  max-width: var(--content-max);
  padding: var(--space-4);
  margin-bottom: var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card);
}

.reconnect__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.reconnect__notice {
  color: var(--warn);
}
</style>
