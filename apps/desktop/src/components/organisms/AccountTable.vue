<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { roleLabel } from "@/accounts/messages";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import HTag from "@/components/atoms/HTag.vue";
import HTooltip from "@/components/atoms/HTooltip.vue";
import { formatDate, formatDateTime } from "@/composables/format";
import { t } from "@/i18n";
import type { Account, Role } from "@/link";

// Tableau des comptes (administrateurs). Chaque action exige le lien ET le rôle administrateur
// (`needs-link`, BR-RESIL-008) : le bouton se désactive et dit pourquoi ; l'agent reste l'arbitre.
// Sur sa propre ligne : « Changer mon mot de passe » seulement (design). Le dernier administrateur
// ne peut être ni rétrogradé ni supprimé : les deux boutons sont grisés avec l'explication.
// `meId` : l'identifiant de l'AGENT du compte de la session (jamais une comparaison de texte avec
// ce qui a été tapé à la connexion). `busy` : une action est en cours, aucune autre ne part.
// `rightsKnown` : l'agent annonce le droit « gérer les services » (sans lui, personne n'est protégé à l'écran).
const props = defineProps<{
  accounts: readonly Account[];
  meId: string;
  busy?: boolean;
  rightsKnown?: boolean;
}>();

const emit = defineEmits<{
  changeRole: [account: Account, role: Role];
  changePassword: [account: Account];
  changeOwnPassword: [];
  closeSessions: [account: Account];
  remove: [account: Account];
  /** Ouvre le détail du compte (lecture seule : le droit de gérer les services y est dit, HRT-78). */
  detail: [account: Account];
}>();

const ADMIN = { role: "admin" } as const;
const OPTIONS: Array<{ value: Role; label: string }> = [
  { value: "admin", label: roleLabel("admin") },
  { value: "readonly", label: roleLabel("readonly") },
];

const admins = computed(() => props.accounts.filter((account) => account.role === "admin").length);
const isLastAdmin = (account: Account) => account.role === "admin" && admins.value <= 1;

// BR-GRANT-005 : le compte d'un titulaire du droit n'est supprimé que par un titulaire (ou la commande sur le
// serveur). L'agent reste l'arbitre : ce geste n'est qu'indisponible ici, avec son explication.
const iHoldTheRight = computed(
  () => props.accounts.find((account) => account.id === props.meId)?.managesServices === true,
);
const protectedFromMe = (account: Account) =>
  props.rightsKnown === true && account.managesServices && !iHoldTheRight.value;
function removeHint(account: Account): string | undefined {
  if (isLastAdmin(account)) return t("accounts.lastAdmin");
  return protectedFromMe(account) ? t("accounts.holderNoDelete") : undefined;
}

const root = ref<HTMLElement | null>(null);
const editing = ref<string | null>(null);

async function startEditing(account: Account) {
  editing.value = account.id;
  await nextTick();
  root.value?.querySelector<HTMLSelectElement>("[data-editing] select")?.focus();
}

// FIX:01M4EHYPD658X22RH4SFV0TDVE : Échap referme la liste et rend le curseur au bouton « Changer le rôle » d'où elle est partie.
async function cancelEditing(account: Account) {
  editing.value = null;
  await nextTick();
  root.value
    ?.querySelector<HTMLElement>(
      `tr[data-account="${account.username}"] button[aria-label^="${t("accounts.changeRole")}"]`,
    )
    ?.focus();
}

// Un compte au rôle que ce client ne connaît pas (agent plus récent) : aucun changement de rôle proposé sur cette base.
function changeRoleHint(account: Account): string | undefined {
  if (account.role === "unknown") return t("accounts.roleUnknownHint");
  return isLastAdmin(account) ? t("accounts.lastAdmin") : undefined;
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
          <th scope="col" class="table__created">{{ t("accounts.colCreated") }}</th>
          <th scope="col">{{ t("accounts.colLastLogin") }}</th>
          <th scope="col" class="table__num">{{ t("accounts.colSessions") }}</th>
          <th scope="col"><span class="sr-only">{{ t("accounts.colActions") }}</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="account in accounts" :key="account.id" :data-account="account.username">
          <th scope="row">
            <div class="table__name">
              <button
                type="button"
                class="table__open"
                :aria-label="t('rights.detailOf', { username: account.username })"
                aria-haspopup="dialog"
                @click="emit('detail', account)"
              >
                {{ account.username }}
              </button>
              <HTag v-if="account.id === meId" tone="neutral">{{ t("accounts.you") }}</HTag>
            </div>
            <span class="table__hint">{{
              t("accounts.createdOn", { date: formatDate(account.createdAt) })
            }}</span>
          </th>
          <td :data-editing="editing === account.id ? '' : undefined">
            <div v-if="editing === account.id" @focusout="editing = null" @keydown.esc="cancelEditing(account)">
              <HSelect
                :model-value="account.role"
                :label="t('accounts.role')"
                :options="OPTIONS"
                hide-label
                @update:model-value="chooseRole(account, $event)"
              />
            </div>
            <div v-else class="table__tags">
              <HTag :tone="account.role === 'admin' ? 'accent' : 'neutral'">
                {{ roleLabel(account.role) }}
              </HTag>
              <!-- BR-GRANT-013 : en lecture seule, aucun geste ne donne ni ne retire ce droit depuis cet écran. -->
              <HTooltip v-if="account.managesServices" :text="t('rights.tagTip')" placement="start">
                <template #default="{ describedby }">
                  <HTag
                    tone="neutral"
                    interactive
                    class="table__right"
                    aria-haspopup="dialog"
                    :aria-describedby="describedby"
                    data-services-right
                    @click="emit('detail', account)"
                  >
                    <HIcon name="box" size="sm" />
                    <span class="table__right-text">{{ t("rights.tag") }}</span>
                  </HTag>
                </template>
              </HTooltip>
            </div>
          </td>
          <td class="table__created">{{ formatDate(account.createdAt) }}</td>
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
                  :disabled="busy || isLastAdmin(account) || account.role === 'unknown'"
                  :hint="changeRoleHint(account)"
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
                  :hint="account.sessionsOpen === 0 ? t('accounts.noSessionToClose') : undefined"
                  :aria-label="named(sessionsLabel(account), account)"
                  @click="emit('closeSessions', account)"
                >
                  {{ sessionsLabel(account) }}
                </HButton>
                <HButton
                  size="sm"
                  variant="danger"
                  :needs-link="ADMIN"
                  :disabled="busy || isLastAdmin(account) || protectedFromMe(account)"
                  :hint="removeHint(account)"
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
  container: accounts / inline-size;
  max-width: var(--table-max);
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
  color: var(--tx-secondary);
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

/* Le nom ouvre le détail du compte : un bouton, reconnaissable comme un lien. */
.table__open {
  padding: 0;
  border: 0;
  background: none;
  color: inherit;
  cursor: pointer;
  font: inherit;
  text-align: left;
  text-decoration: underline;
  text-decoration-color: var(--tx-secondary);
  text-underline-offset: var(--space-half);
}

.table__open:hover {
  color: var(--ac);
  text-decoration-color: currentColor;
}

.table__tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}

.table__right {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

/* FIX:01M4DNJ3W899Y50NPSRQ8W8ZVN (C26) : l'en-tête garde la police des autres titres. */
.table thead .table__num {
  font-family: inherit;
}

.table__num {
  text-align: right;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

/* HRT-44 : la date de création, cachée tant que la colonne « Créé le » est affichée. */
.table__hint {
  display: none;
}

/* FIX:01M4DNJ3P75DN25714EBCH2EY2 (C22) : la cellule du rôle en cours de changement se distingue des autres lignes (fond et trait). */
.table td[data-editing] :deep(.select__input) {
  width: auto;
  min-width: 100%;
}

.table td[data-editing] {
  background: var(--card-2);
  box-shadow: inset 0 0 0 var(--ring-width) var(--ac);
}

.table__actions {
  display: flex;
  flex-wrap: nowrap;
  white-space: nowrap;
  justify-content: flex-end;
  gap: var(--space-2);
}

/* FIX:01M4DKQWTWVNDXFPWJ5SE88SN7 (C21) : tableau de moins de 1100 px de large (fenêtre de 1100 à 1366 px) : la date de création, la donnée la moins
   utile, cède sa place aux actions, qui tiennent sur une ligne et restent entièrement visibles (HRT-36, C21).
   Plus large, elle reste affichée. */
@container accounts (max-width: 1100px) {
  .table__created {
    display: none;
  }

  /* La date de création, absente de sa colonne : petit texte atténué sous l'identifiant (jamais dite deux fois). */
  .table__hint {
    display: block;
    color: var(--tx3);
    font-size: var(--fs-small);
    font-weight: var(--fw-regular);
    white-space: nowrap;
  }

  .table th,
  .table td {
    padding-inline: var(--space-2);
  }
}

/* Tableau de moins de 1000 px (fenêtre de 1280 px) : le repère « Peut gérer les services » (texte long) élargirait la colonne du rôle
   de 30 px de trop, et « Changer mon mot de passe » sortirait du tableau. Le repère garde son icône ; son texte reste lu
   par les lecteurs d'écran et en infobulle, et il se dit en entier dès que la place existe (et dans le détail du compte). */
@container accounts (max-width: 1000px) {
  .table__right-text {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
}

/* Fenêtre minimale (tableau de moins de 850 px) : même en retirant la date il n'y a plus la place pour une ligne ;
   les actions passent alors à la ligne DANS leur cellule, sans jamais sortir du tableau. */
@container accounts (max-width: 850px) {
  .table__actions {
    flex-wrap: wrap;
    white-space: normal;
  }
}
</style>
