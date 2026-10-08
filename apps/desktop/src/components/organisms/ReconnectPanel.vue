<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import LoginForm from "@/components/organisms/LoginForm.vue";
import { useReconnect } from "@/composables/useReconnect";
import { t } from "@/i18n";
import type { Reason, ServerInfo } from "@/link";

// Serveur enregistré sans session : session expirée, déconnexion volontaire, première connexion
// interrompue, mot de passe mémorisé devenu invalide, accès révoqué. Panneau non bloquant au-dessus
// de la dernière vue (périmée) : jamais de fenêtre modale (BR-RESIL-011). Un mot de passe mémorisé
// qui marche ne passe jamais par ici : la bibliothèque rouvre la session en silence (BR-RESIL-013).
// Session expirée : « Ta session a expiré. » puis le mot de passe à ressaisir (« Me reconnecter »).
// Mot de passe mémorisé refusé : juste le formulaire (BR-CONN-017). Accès révoqué : « Ton compte
// n'est plus accessible. », et « Utiliser un autre compte » ouvre le formulaire, identifiant vide
// (BR-RESIL-014). Le panneau est recréé pour chaque serveur (`:key` du gabarit) : la saisie d'un
// serveur ne part jamais vers un autre.
const props = defineProps<{ server: ServerInfo; reason: Reason | null; revoked?: boolean }>();

const reconnect = useReconnect(() => props.server);
const form = ref<InstanceType<typeof LoginForm> | null>(null);
const otherAccount = ref(false);

const expired = computed(() => !props.revoked && props.reason === "expired");
const notice = computed(() => {
  if (props.revoked) return t("link.revokedNotice");
  return expired.value ? t("link.sessionExpiredNotice") : null;
});
const hint = computed(() => {
  if (props.revoked) return t("link.revokedHint");
  if (expired.value || props.reason === "stored_password_refused") return t("link.askPassword");
  return null;
});
const showForm = computed(() => !props.revoked || otherAccount.value);

// FIX:01M4D0RHZE7JFMV700JKA3DM1R (C56) : à l'ouverture du panneau (session expirée, accès révoqué puis
// « Utiliser un autre compte »), le curseur est dans le premier champ à remplir : le mot de passe quand
// l'identifiant est connu, l'identifiant sinon ; après un refus il revient dans le mot de passe vidé.
async function focusForm() {
  await nextTick();
  form.value?.focusFirstEmpty();
}
// À l'ouverture seulement si le curseur n'est dans aucun champ : ce que l'utilisateur est en train de taper
// ailleurs (une autre page, un autre serveur) n'est jamais volé par un panneau qui apparaît.
function cursorInAField(): boolean {
  const active = document.activeElement;
  return (
    active instanceof HTMLInputElement ||
    active instanceof HTMLTextAreaElement ||
    active instanceof HTMLSelectElement ||
    (active instanceof HTMLElement && active.isContentEditable)
  );
}
onMounted(() => {
  if (!cursorInAField()) void focusForm();
});
watch(showForm, (shown) => {
  if (shown) void focusForm();
});

async function submit(entry: { username: string; password: string; remember: boolean }) {
  const connected = await reconnect.submit(entry);
  if (!connected) {
    form.value?.clearPassword();
    void focusForm();
  }
}
</script>

<template>
  <section class="reconnect" :aria-label="t('connect.reconnectTitle', { name: server.name })">
    <h2 class="reconnect__title">{{ t("connect.reconnectTitle", { name: server.name }) }}</h2>
    <p v-if="notice" class="reconnect__notice" role="status">{{ notice }}</p>
    <p v-if="hint" class="reconnect__hint">{{ hint }}</p>
    <HButton v-if="revoked && !otherAccount" variant="secondary" @click="otherAccount = true">
      {{ t("link.useAnotherAccount") }}
    </HButton>
    <LoginForm
      v-if="showForm"
      ref="form"
      :username="revoked ? '' : server.username"
      :busy="reconnect.busy.value"
      :error="reconnect.error.value"
      :locked-seconds="reconnect.lockedSeconds.value"
      :submit-label="expired ? t('link.reconnectAction') : undefined"
      :remember="server.remember || reason !== 'stored_password_refused'"
      @submit="submit"
    />
  </section>
</template>

<style scoped>
.reconnect {
  /* FIX:01M4D4FQRZ2QTRT57HMM6TAPBE : en surimpression, centré, sans repousser la dernière vue (revue UX C45) */
  position: absolute;
  inset: 0;
  z-index: var(--z-tooltip);
  height: fit-content;
  box-shadow: var(--card-edge);
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  width: min(100%, var(--content-max));
  padding: var(--space-4);
  margin: auto;
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

.reconnect__hint {
  color: var(--tx2);
}
</style>
