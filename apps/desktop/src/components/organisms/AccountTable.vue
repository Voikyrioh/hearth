<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { roleLabel } from "@/accounts/messages";
import HButton from "@/components/atoms/HButton.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import HTag from "@/components/atoms/HTag.vue";
import { formatDate, formatDateTime } from "@/composables/format";
import { t } from "@/i18n";
import type { Account, Role } from "@/link";

// Tableau des comptes (administrateurs). Chaque action exige le lien ET le rôle administrateur
// (`needs-link`, BR-RESIL-008) : le bouton se désactive et dit pourquoi ; l'agent reste l'arbitre.
// Sur sa propre ligne : « Changer mon mot de passe » seulement (design). Le dernier administrateur
// ne peut être ni rétrogradé ni supprimé : les deux boutons sont grisés avec l'explication.
// `meId` : l'identifiant de l'AGENT du compte de la session (jamais une comparaison de texte avec
// ce qui a été tapé à la connexion). `busy` : une action est en cours, aucune autre ne part.
const props = defineProps<{ accounts: readonly Account[]; meId: string; busy?: boolean }>();

const emit = defineEmits<{
  changeRole: [account: Account, role: Role];
  changePassword: [account: Account];
  changeOwnPassword: [];
  closeSessions: [account: Account];
  remove: [account: Account];
}>();

const ADMIN = { role: "admin" } as const;
const OPTIONS: Array<{ value: Role; label: string }> = [
  { value: "admin", label: roleLabel("admin") },
  { value: "readonly", label: roleLabel("readonly") },
];

const admins = computed(() => props.accounts.filter((account) => account.role === "admin").length);
const isLastAdmin = (account: Account) => account.role === "admin" && admins.value <= 1;

const root = ref<HTMLElement | null>(null);
const editing = ref<string | null>(null);

async function startEditing(account: Account) {
  editing.value = account.id;
  await nextTick();
  root.value?.querySelector<HTMLSelectElement>("[data-editing] select")?.focus();
}

function chooseRole(account: Account, role: Role) {
  editing.value = null;
  if (role !== account.role) emit("changeRole", account, role);
}

function sessionsLabel(account: Account): string {
  return account.sessionsOpen >= 2
    ? t("accounts.closeSessionsMany", { n: account.sessionsOpen })
    : t("accounts.closeSessions");
}

const named = (label: string, account: Account) => `${label} ${account.username}`;
</script>

<template>
  <div ref="root" class="table-wrap">
    <table class="table">
      <thead>
        <tr>
          <th scope="col">{{ t("accounts.colUsername") }}</th>
          <th scope="col">{{ t("accounts.colRole") }}</th>
          <th scope="col">{{ t("accounts.colCreated") }}</th>
          <th scope="col">{{ t("accounts.colLastLogin") }}</th>
          <th scope="col" class="table__num">{{ t("accounts.colSessions") }}</th>
          <th scope="col"><span class="sr-only">{{ t("accounts.colActions") }}</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="account in accounts" :key="account.id" :data-account="account.username">
          <th scope="row">
            <div class="table__name">
              {{ account.username }}
              <HTag v-if="account.id === meId" tone="neutral">{{ t("accounts.you") }}</HTag>
            </div>
          </th>
          <td :data-editing="editing === account.id ? '' : undefined">
            <div v-if="editing === account.id" @focusout="editing = null" @keydown.esc="editing = null">
              <HSelect
                :model-value="account.role"
                :label="t('accounts.role')"
                :options="OPTIONS"
                hide-label
                @update:model-value="chooseRole(account, $event)"
              />
            </div>
            <HTag v-else :tone="account.role === 'admin' ? 'accent' : 'neutral'">
              {{ roleLabel(account.role) }}
            </HTag>
          </td>
          <td>{{ formatDate(account.createdAt) }}</td>
          <td>{{ account.lastLoginAt ? formatDateTime(account.lastLoginAt) : t("accounts.neverLoggedIn") }}</td>
          <td class="table__num">{{ account.sessionsOpen }}</td>
          <td>
            <div class="table__actions">
              <template v-if="account.id === meId">
                <HButton
                  size="sm"
                  variant="secondary"
                  needs-link
                  :disabled="busy"
                  @click="emit('changeOwnPassword')"
                >
                  {{ t("accounts.changeOwnPassword") }}
                </HButton>
              </template>
              <template v-else>
                <HButton
                  size="sm"
                  variant="secondary"
                  :needs-link="ADMIN"
                  :disabled="busy || isLastAdmin(account)"
                  :hint="isLastAdmin(account) ? t('accounts.lastAdmin') : undefined"
                  :aria-label="named(t('accounts.changeRole'), account)"
                  @click="startEditing(account)"
                >
                  {{ t("accounts.changeRole") }}
                </HButton>
                <HButton
                  size="sm"
                  variant="secondary"
                  :needs-link="ADMIN"
                  :disabled="busy"
                  :aria-label="named(t('accounts.password'), account)"
                  @click="emit('changePassword', account)"
                >
                  {{ t("accounts.password") }}
                </HButton>
                <HButton
                  size="sm"
                  variant="secondary"
                  :needs-link="ADMIN"
                  :disabled="busy || account.sessionsOpen === 0"
                  :aria-label="named(sessionsLabel(account), account)"
                  @click="emit('closeSessions', account)"
                >
                  {{ sessionsLabel(account) }}
                </HButton>
                <HButton
                  size="sm"
                  variant="danger"
                  :needs-link="ADMIN"
                  :disabled="busy || isLastAdmin(account)"
                  :hint="isLastAdmin(account) ? t('accounts.lastAdmin') : undefined"
                  :aria-label="named(t('accounts.remove'), account)"
                  @click="emit('remove', account)"
                >
                  {{ t("accounts.remove") }}
                </HButton>
              </template>
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.table-wrap {
  overflow-x: auto;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.table {
  width: 100%;
  border-collapse: collapse;
}

.table th,
.table td {
  padding: var(--space-3) var(--space-4);
  border-bottom: var(--border-width) solid var(--bd);
  text-align: left;
  vertical-align: middle;
}

.table thead th {
  color: var(--tx2);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
  letter-spacing: var(--ls-label);
}

.table tbody tr:last-child th,
.table tbody tr:last-child td {
  border-bottom: 0;
}

.table__name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-weight: var(--fw-semibold);
}

.table__num {
  text-align: right;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.table__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-2);
}
</style>
