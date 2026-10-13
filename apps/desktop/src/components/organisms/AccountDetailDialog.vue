<script setup lang="ts">
import { computed } from "vue";
import { roleLabel } from "@/accounts/messages";
import FormDialog from "@/components/molecules/FormDialog.vue";
import ServicesRightRow from "@/components/molecules/ServicesRightRow.vue";
import { formatDate, formatDateTime } from "@/composables/format";
import { t } from "@/i18n";
import type { Account } from "@/link";

// Le détail d'un compte (HRT-78, BR-GRANT-013, écran I) : ce que la liste dit déjà, et la ligne « Gestion des
// services » avec l'aide et la commande à lancer sur le serveur. EN LECTURE SEULE : aucun bouton n'agit ici
// (les actions sur un compte restent dans la ligne de la liste), et rien ne laisse croire que le droit se donne
// depuis cet écran (BR-GRANT-005). La fenêtre ne propose que de la quitter.
const props = defineProps<{
  account: Account | null;
  /** L'agent annonce `service-rights` (BR-GRANT-016). */
  rightsKnown: boolean;
  /** L'identifiant de l'agent du compte de la session : sur sa propre ligne, retirer le droit est signalé. */
  meId?: string;
  /** Pourquoi ce compte ne peut pas être supprimé par toi, dit en clair (jamais seulement au survol). */
  removalReason?: string;
}>();
const emit = defineEmits<{ close: [] }>();

const title = computed(() =>
  props.account ? t("rights.detailOf", { username: props.account.username }) : "",
);
</script>

<template>
  <FormDialog
    :open="account !== null"
    :title="title"
    submit-label=""
    hide-submit
    :cancel-label="t('common.close')"
    @cancel="emit('close')"
    @submit="emit('close')"
  >
    <template v-if="account">
      <dl class="detail" data-account-detail>
        <div class="detail__line">
          <dt>{{ t("accounts.colRole") }}</dt>
          <dd>{{ roleLabel(account.role) }}</dd>
        </div>
        <div class="detail__line">
          <dt>{{ t("accounts.colCreated") }}</dt>
          <dd>{{ formatDate(account.createdAt) }}</dd>
        </div>
        <div class="detail__line">
          <dt>{{ t("accounts.colLastLogin") }}</dt>
          <dd>
            {{
              account.lastLoginAt
                ? formatDateTime(account.lastLoginAt)
                : t("accounts.neverLoggedIn")
            }}
          </dd>
        </div>
        <div class="detail__line">
          <dt>{{ t("accounts.colSessions") }}</dt>
          <dd>{{ account.sessionsOpen }}</dd>
        </div>
      </dl>
      <p v-if="removalReason" class="detail__note" data-removal-reason>{{ removalReason }}</p>
      <ServicesRightRow
        :username="account.username"
        :manages="account.managesServices"
        :known="rightsKnown"
        :own="account.id === meId"
      />
    </template>
  </FormDialog>
</template>

<style scoped>
.detail {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
}

.detail__line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-4);
}

.detail__note {
  margin: 0;
  color: var(--tx2);
  font-size: var(--fs-small);
}

.detail__line dt {
  color: var(--tx2);
}

.detail__line dd {
  margin: 0;
  font-weight: var(--fw-semibold);
  text-align: right;
}
</style>
