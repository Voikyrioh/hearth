<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { refusalMessage } from "@/accounts/messages";
import HButton from "@/components/atoms/HButton.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import AccountTable from "@/components/organisms/AccountTable.vue";
import CreateAccountDialog from "@/components/organisms/CreateAccountDialog.vue";
import PasswordDialog from "@/components/organisms/PasswordDialog.vue";
import { useAccountActions } from "@/composables/useAccountActions";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { t } from "@/i18n";
import type { Account, Role } from "@/link";
import { useAccountsStore } from "@/stores/accounts";
import { useLinkStore } from "@/stores/link";

// Comptes d'un serveur (administrateurs : l'entrée de menu n'existe pas pour Lecture seule et le
// routeur ferme la route ; l'agent refuse de toute façon, et le refus s'affiche proprement ici).
// Toute action passe par `useAccountActions` (donc `useServerAction` : désactivée et expliquée hors
// « Connecté », résultat inconnu à la coupure, jamais rejouée). La liste se relit à chaque retour du
// lien et à chaque issue d'opération incertaine : jamais d'état supposé.
const { server, state } = useCurrentServer();
const store = useAccountsStore();
const link = useLinkStore();
const serverId = computed(() => server.value?.id ?? "");
const actions = useAccountActions(() => serverId.value);

const entry = computed(() => store.of(serverId.value));
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
const removeError = ref<string | undefined>();

async function changeRole(account: Account, role: Role) {
  const report = await actions.changeRole(account, role);
  actions.toastRefusal(report);
}

async function closeSessions(account: Account) {
  const report = await actions.closeSessions(account);
  actions.toastRefusal(report);
}

function askRemove(account: Account) {
  removeError.value = undefined;
  removing.value = account;
}

async function confirmRemove() {
  const account = removing.value;
  if (!account) return;
  const report = await actions.remove(account);
  if (report.kind === "refused") {
    // Dernier administrateur : on le dit, la fenêtre se ferme, le compte est inchangé. Autre erreur :
    // dans la fenêtre, annuler ou réessayer.
    if (report.refusal.kind === "last_admin") {
      actions.toastRefusal(report);
      removing.value = null;
    } else {
      removeError.value = refusalMessage(report.refusal);
    }
    return;
  }
  if (report.kind !== "failed") removing.value = null;
}
</script>

<template>
  <Teleport defer to="#header-actions">
    <HButton :needs-link="{ role: 'admin' }" tip-placement="end" @click="creating = true">
      {{ t("pages.addAccount") }}
    </HButton>
  </Teleport>

  <p v-if="entry?.status === 'refused'" class="accounts__notice" role="alert">
    {{ entry.refusal ? refusalMessage(entry.refusal) : t("accounts.forbidden") }}
  </p>
  <div v-else-if="!entry || (entry.status === 'loading' && accounts.length === 0)" class="accounts__wait">
    <HSpinner :label="t('common.loading')" />
  </div>
  <div v-else-if="entry.status === 'error' && accounts.length === 0" class="accounts__notice">
    <p role="alert">{{ t("accounts.loadFailed") }}</p>
    <HButton variant="secondary" @click="store.load(serverId)">{{ t("common.retry") }}</HButton>
  </div>
  <section v-else-if="accounts.length === 0" class="accounts__empty">
    <EmptyState :title="t('pages.accounts')" :text="t('accounts.empty')" heading="h2">
      <template #action>
        <HButton :needs-link="{ role: 'admin' }" @click="creating = true">
          {{ t("pages.addAccount") }}
        </HButton>
      </template>
    </EmptyState>
  </section>
  <AccountTable
    v-else
    :accounts="accounts"
    :me-id="meId"
    :busy="actions.busy.value"
    @change-role="changeRole"
    @change-password="passwordFor = $event"
    @change-own-password="ownPassword = true"
    @close-sessions="closeSessions"
    @remove="askRemove"
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
  <ConfirmDialog
    :open="removing !== null"
    :title="t('accounts.removeTitle')"
    :message="t('accounts.removeMessage', { username: removing?.username ?? '' })"
    :confirm-label="t('accounts.remove')"
    :busy="actions.busy.value"
    :error="removeError"
    destructive
    @confirm="confirmRemove"
    @cancel="removing = null"
  />
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
</style>
