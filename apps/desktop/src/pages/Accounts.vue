<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage, roleLabel } from "@/accounts/messages";
import { SCREEN_ILLUSTRATIONS } from "@/assets/illustrations/screens";
import HButton from "@/components/atoms/HButton.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import AccountTable from "@/components/organisms/AccountTable.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import CreateAccountDialog from "@/components/organisms/CreateAccountDialog.vue";
import PasswordDialog from "@/components/organisms/PasswordDialog.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { usePageData } from "@/composables/usePageData";
import { t } from "@/i18n";
import type { Account, Role } from "@/link";
import { useAccountsStore } from "@/stores/accounts";
import { useLinkStore } from "@/stores/link";

// Comptes d'un serveur (administrateurs : l'entrée de menu n'existe pas pour Lecture seule et le
// routeur ferme la route ; l'agent refuse de toute façon, et le refus s'affiche proprement ici).
// Toute action passe par `useAccountActions` (donc `useServerAction` : désactivée et expliquée hors
// « Connecté », résultat inconnu à la coupure, jamais rejouée). La liste se relit à chaque retour du
// lien et à chaque issue d'opération incertaine : jamais d'état supposé.
const { server, state, isConnected } = useCurrentServer();
const store = useAccountsStore();
const link = useLinkStore();
const serverId = computed(() => server.value?.id ?? "");
const actions = useAccountActions(() => serverId.value);

const entry = computed(() => store.of(serverId.value));
// FIX:01M4E5D4JY66T0ETEMRY1DZQZK (C46) : sans liste lue, la page n'a rien à dater (pas de « Vu il y a… »).
usePageData(() => entry.value?.status === "ready" || (entry.value?.accounts.length ?? 0) > 0);
const accounts = computed(() => entry.value?.accounts ?? []);
const meId = computed(() => entry.value?.me ?? "");
const myUsername = computed(() => server.value?.username ?? "");

watch(
  serverId,
  (id) => {
    if (id) void store.load(id);
  },
  { immediate: true },
);
// Retour du lien : l'état réel du serveur (BR-RESIL-010).
watch(state, (now, before) => {
  if (now === "connected" && before !== "connected" && serverId.value) {
    void store.load(serverId.value);
  }
});
// Une issue d'opération connue (« fait pendant la coupure », « non exécuté ») : relire.
watch(
  () => Object.keys(link.operations).length,
  () => {
    if (serverId.value) void store.load(serverId.value);
  },
);

const creating = ref(false);
const passwordFor = ref<Account | null>(null);
const ownPassword = ref(false);
const removing = ref<Account | null>(null);
const roleChange = ref<{ account: Account; role: Role } | null>(null);
const closing = ref<Account | null>(null);

// Changer un rôle, fermer des sessions et supprimer un compte sont des actes d'administration : chacun
// passe par la fenêtre commune de confirmation (mot de passe selon ce que l'agent annonce, clé de ce PC
// vérifiée par la coquille). Les trois restent ouvertes sur un refus, sauf « dernier administrateur » :
// on le dit, la fenêtre se ferme, le compte est inchangé.
async function performRole(adminPassword: string | null) {
  const change = roleChange.value;
  if (!change) return { kind: "failed" as const };
  const report = await actions.changeRole(change.account, change.role, adminPassword);
  if (report.kind === "refused" && report.refusal.kind === "last_admin") {
    actions.toastRefusal(report);
    return { kind: "done" as const };
  }
  return report;
}

async function performClose(adminPassword: string | null) {
  const account = closing.value;
  if (!account) return { kind: "failed" as const };
  return actions.closeSessions(account, adminPassword);
}

async function performRemove(adminPassword: string | null) {
  const account = removing.value;
  if (!account) return { kind: "failed" as const };
  const report = await actions.remove(account, null, adminPassword);
  if (report.kind === "refused" && report.refusal.kind === "last_admin") {
    actions.toastRefusal(report);
    return { kind: "done" as const };
  }
  return report;
}

const refusalText = (refusal: { kind: string }) => refusalMessage(refusal as never);
</script>

<template>
  <Teleport defer to="#header-actions">
    <HButton :needs-link="{ role: 'admin' }" tip-placement="end" @click="creating = true">
      {{ t("pages.addAccount") }}
    </HButton>
  </Teleport>

  <p v-if="entry?.status === 'refused'" class="accounts__notice" role="alert">
    {{ t("accounts.forbidden") }}
  </p>
  <div v-else-if="!entry || (entry.status === 'loading' && accounts.length === 0)" class="accounts__wait">
    <HSpinner :label="t('common.loading')" />
  </div>
  <!-- Serveur injoignable : « pas encore chargé », sans second « Réessayer » (le bandeau a le sien) ; HRT-38 (C46). -->
  <p
    v-else-if="entry.status === 'error' && accounts.length === 0 && !isConnected"
    class="accounts__notice accounts__notice--pending"
    data-not-loaded-yet
  >
    {{ t("link.notLoadedYet") }}
  </p>
  <div v-else-if="entry.status === 'error' && accounts.length === 0" class="accounts__notice">
    <p role="alert">{{ t("accounts.loadFailed") }}</p>
    <HButton variant="secondary" @click="store.load(serverId)">{{ t("common.retry") }}</HButton>
  </div>
  <section v-else-if="accounts.length === 0" class="accounts__empty">
    <!-- FIX:01M4D4FSDN82D1PNC7YQEERDNB : le titre de la page et son bouton d'action (en-tête) suffisent. -->
    <EmptyState
      :title="t('accounts.emptyTitle')"
      :text="t('accounts.empty')"
      heading="h2"
      :illustration="SCREEN_ILLUSTRATIONS.accounts ?? undefined"
    />
  </section>
  <AccountTable
    v-else
    :accounts="accounts"
    :me-id="meId"
    :busy="actions.busy.value"
    @change-role="(account, role) => (roleChange = { account, role })"
    @change-password="passwordFor = $event"
    @change-own-password="ownPassword = true"
    @close-sessions="closing = $event"
    @remove="removing = $event"
  />

  <CreateAccountDialog :open="creating" :server-id="serverId" @close="creating = false" />
  <PasswordDialog
    :open="passwordFor !== null"
    :server-id="serverId"
    :username="passwordFor?.username ?? ''"
    :own="false"
    :account="passwordFor"
    @close="passwordFor = null"
  />
  <PasswordDialog
    :open="ownPassword"
    :server-id="serverId"
    :username="myUsername"
    own
    @close="ownPassword = false"
  />
  <AdminActDialog
    :open="roleChange !== null"
    :server-id="serverId"
    kind="account_role"
    :role="roleChange?.role ?? null"
    :title="t('reauth.roleTitle', { username: roleChange?.account.username ?? '' })"
    :submit-label="t('reauth.roleApply')"
    :perform="performRole"
    :refusal-text="refusalText"
    @close="roleChange = null"
  >
    <p data-role-message>
      {{
        t("reauth.roleMessage", {
          username: roleChange?.account.username ?? "",
          role: roleChange ? roleLabel(roleChange.role) : "",
        })
      }}
    </p>
  </AdminActDialog>
  <AdminActDialog
    :open="closing !== null"
    :server-id="serverId"
    kind="sessions_revoke"
    :title="t('reauth.sessionsTitle', { username: closing?.username ?? '' })"
    :submit-label="t('reauth.sessionsApply')"
    :perform="performClose"
    :refusal-text="refusalText"
    @close="closing = null"
  >
    <p data-sessions-message>{{ t("reauth.sessionsMessage", { username: closing?.username ?? "" }) }}</p>
  </AdminActDialog>
  <AdminActDialog
    :open="removing !== null"
    :server-id="serverId"
    kind="account_delete"
    :title="t('accounts.removeTitle')"
    :submit-label="t('accounts.remove')"
    destructive
    :perform="performRemove"
    :refusal-text="refusalText"
    @close="removing = null"
  >
    <p data-remove-message>{{ t("accounts.removeMessage", { username: removing?.username ?? "" }) }}</p>
  </AdminActDialog>
</template>

<style scoped>
.accounts__wait,
.accounts__empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: var(--soon-min-height);
  padding: var(--space-6) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.accounts__notice {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
  color: var(--crit);
}

.accounts__notice--pending {
  color: var(--tx2);
}
</style>
