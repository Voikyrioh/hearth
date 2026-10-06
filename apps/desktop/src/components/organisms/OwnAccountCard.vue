<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage, roleLabel } from "@/accounts/messages";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HTag from "@/components/atoms/HTag.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import PasswordDialog from "@/components/organisms/PasswordDialog.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { t } from "@/i18n";
import { getLinkBridge, type ServerInfo } from "@/link";
import { useAccountsStore } from "@/stores/accounts";
import { useLinkStore } from "@/stores/link";

// Mon compte sur UN serveur (réglages, tous les rôles) : changer son mot de passe (ferme les autres
// sessions, garde celle-ci) et, pour un administrateur, supprimer son propre compte (confirmation
// renforcée : retaper son identifiant, BR-ACCT-012). Les boutons exigent le lien de CE serveur
// (`needs-link` avec `server`). Le rôle affiché est celui de la dernière connexion ; l'agent reste
// l'arbitre (le refus du dernier administrateur est dit tel quel).
const props = defineProps<{ server: ServerInfo }>();

const accounts = useAccountsStore();
const link = useLinkStore();
const connected = computed(() => link.stateOf(props.server.id) === "connected");
const actions = useAccountActions(() => props.server.id);

const changing = ref(false);
const removing = ref(false);
const retyped = ref("");
const error = ref<string | undefined>();
/** Toute la séquence (relecture de la liste PUIS suppression) : un second envoi n'est pas possible. */
const working = ref(false);

watch(removing, () => {
  retyped.value = "";
  error.value = undefined;
});

async function confirmRemove() {
  if (working.value) return;
  working.value = true;
  try {
    await remove();
  } finally {
    working.value = false;
  }
}

async function remove() {
  error.value = undefined;
  // Mon compte = celui que l'AGENT donne pour cette session (jamais une comparaison de texte).
  await accounts.load(props.server.id);
  const entry = accounts.of(props.server.id);
  const mine = entry?.accounts.find((account) => account.id === entry.me);
  if (!mine) {
    error.value = refusalMessage({ kind: "not_found" });
    return;
  }
  const report = await actions.remove(mine, retyped.value);
  retyped.value = "";
  if (report.kind === "refused") {
    error.value = refusalMessage(report.refusal, true);
    return;
  }
  if (report.kind === "failed") return;
  removing.value = false;
  if (report.kind === "done") {
    // Le compte n'existe plus : ce mot de passe mémorisé ne sert plus à rien (au mieux).
    void getLinkBridge()
      .forgetCredentials(props.server.id)
      .catch(() => {});
  }
}
</script>

<template>
  <section class="mine" :data-server="server.id">
    <header class="mine__head">
      <h3 class="mine__title">{{ server.name }}</h3>
      <HTag :tone="server.role === 'admin' ? 'accent' : 'neutral'">{{ roleLabel(server.role) }}</HTag>
    </header>
    <p class="mine__as">
      {{ t(connected ? "settings.connectedAs" : "settings.lastAccount", { username: server.username }) }}
    </p>
    <div class="mine__actions">
      <HButton variant="secondary" :needs-link="{ server: server.id }" @click="changing = true">
        {{ t("accounts.changeOwnPassword") }}
      </HButton>
      <HButton
        v-if="server.role === 'admin'"
        variant="danger"
        :needs-link="{ server: server.id, role: 'admin' }"
        @click="removing = true"
      >
        {{ t("accounts.removeOwn") }}
      </HButton>
    </div>

    <PasswordDialog
      :open="changing"
      :server-id="server.id"
      :username="server.username"
      own
      @close="changing = false"
    />
    <FormDialog
      :open="removing"
      :title="t('accounts.removeOwnTitle')"
      :submit-label="t('accounts.removeOwn')"
      :can-submit="retyped.trim() !== ''"
      :busy="working || actions.busy.value"
      :error="error"
      destructive
      @submit="confirmRemove"
      @cancel="removing = false"
    >
      <HInput
        v-model="retyped"
        :label="t('accounts.removeOwnHelp')"
        :placeholder="t('accounts.usernamePlaceholder')"
        autocomplete="off"
      />
    </FormDialog>
  </section>
</template>

<style scoped>
.mine {
  margin-top: var(--space-5);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.mine__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.mine__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.mine__as {
  margin-top: var(--space-2);
  color: var(--tx2);
}

.mine__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin-top: var(--space-4);
}
</style>
