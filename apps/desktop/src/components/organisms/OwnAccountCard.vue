<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage, roleLabel } from "@/accounts/messages";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HTag from "@/components/atoms/HTag.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import PasswordDialog from "@/components/organisms/PasswordDialog.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import type { ActReport } from "@/composables/useReauth";
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

watch(removing, () => {
  retyped.value = "";
});

// Supprimer son compte est un acte d'administration confirmé comme les autres (fenêtre commune : mot de
// passe selon ce que l'agent annonce) EN PLUS de l'identifiant retapé (BR-ACCT-012). Toute la séquence
// (relecture de la liste PUIS suppression) passe par cette fonction : la fenêtre n'envoie qu'une fois.
async function perform(adminPassword: string | null): Promise<ActReport> {
  // Mon compte = celui que l'AGENT donne pour cette session (jamais une comparaison de texte).
  await accounts.load(props.server.id);
  const entry = accounts.of(props.server.id);
  const mine = entry?.accounts.find((account) => account.id === entry.me);
  if (!mine) return { kind: "refused", refusal: { kind: "not_found" } };
  const report = await actions.remove(mine, retyped.value, adminPassword);
  if (!(report.kind === "refused" && report.refusal.kind === "password_required")) {
    retyped.value = "";
  }
  if (report.kind === "done") {
    // Le compte n'existe plus : ce mot de passe mémorisé ne sert plus à rien (au mieux).
    void getLinkBridge()
      .forgetCredentials(props.server.id)
      .catch(() => {});
  }
  return report;
}

const refusalText = (refusal: { kind: string }) => refusalMessage(refusal as never, true);
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
      <HButton
        variant="secondary"
        :needs-link="{ server: server.id }"
        reason-below
        @click="changing = true"
      >
        {{ t("accounts.changeOwnPassword") }}
      </HButton>
      <HButton
        v-if="server.role === 'admin'"
        variant="danger"
        :needs-link="{ server: server.id, role: 'admin' }"
        reason-below
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
    <AdminActDialog
      :open="removing"
      :server-id="server.id"
      kind="account_delete"
      :title="t('accounts.removeOwnTitle')"
      :submit-label="t('accounts.removeOwn')"
      :can-submit="retyped.trim() !== ''"
      destructive
      :perform="perform"
      :refusal-text="refusalText"
      @close="removing = false"
    >
      <HInput
        v-model="retyped"
        :label="t('accounts.removeOwnHelp')"
        :placeholder="t('accounts.usernamePlaceholder')"
        autocomplete="off"
      />
    </AdminActDialog>
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
