<script setup lang="ts">
import { defineAsyncComponent, inject } from "vue";
import { routerKey } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import ReauthField from "@/components/molecules/ReauthField.vue";
import type { AdminAct } from "@/composables/useAdminAct";
import { t } from "@/i18n";

// Le corps de la confirmation d'un acte d'administration (HRT-245) : les champs de l'acte (fente par défaut), l'état de la
// confirmation et « Votre mot de passe » SELON CE QUE L'AGENT ANNONCE. Toute la logique est dans `useAdminAct` ; ce
// panneau n'affiche que son état. `AdminActDialog` le pose dans une fenêtre, la page de vérification (HRT-246) dans la page.

// Mot de passe du compte simulé, rappelé en mode développement seulement (retiré du binaire livré).
const DevPasswordHint = import.meta.env.DEV
  ? defineAsyncComponent(() => import("@/components/organisms/DevPasswordHint.vue"))
  : null;

defineProps<{
  act: AdminAct;
  serverId: string;
  /** Le libellé du champ quand le mot de passe de confirmation a un nom propre (« Ancien mot de passe »). */
  passwordLabel?: string;
}>();
const emit = defineEmits<{ leave: [] }>();

const router = inject(routerKey, null);

/** Mène à la page de mise à jour de l'agent (Réglages) et rend la main à l'enveloppe. */
async function goToSettings() {
  emit("leave");
  await router?.push({ name: "settings" });
}
</script>

<template>
  <div class="admin" data-admin-act-panel>
    <div v-if="act.awaiting.value" class="admin__wait" data-reauth-loading>
      <HSpinner :label="t('reauth.loading')" />
    </div>
    <div v-else-if="act.reauth.failed.value" class="admin__state" data-reauth-failed>
      <p role="alert" class="admin__alert">{{ t("reauth.stateFailed") }}</p>
      <HButton variant="secondary" @click="act.reauth.load()">{{ t("common.retry") }}</HButton>
    </div>
    <div v-else-if="act.reauth.agentTooOld.value" class="admin__state" data-reauth-agent-old>
      <p role="alert" class="admin__alert">{{ t("failure.agentTooOld") }}</p>
      <HButton variant="secondary" data-reauth-settings @click="goToSettings">
        {{ t("reauth.goSettings") }}
      </HButton>
    </div>
    <div v-else-if="act.reauth.keyMissing.value" class="admin__state" data-reauth-no-key>
      <p class="admin__help">{{ t("reauth.noKey") }}</p>
      <HButton variant="secondary" data-reauth-reconnect @click="act.reauth.reconnect()">
        {{ t("reauth.reconnect") }}
      </HButton>
    </div>
    <template v-else>
      <div :ref="(el) => (act.acts.value = el as HTMLElement | null)" class="admin__acts">
        <slot />
      </div>
      <p v-if="act.notice.value" class="admin__notice" role="status" data-reauth-notice>
        {{ act.notice.value }}
      </p>
      <p v-if="act.reauth.elevated.value" class="admin__help" data-reauth-elevated>
        {{ t("reauth.elevated", { time: act.reauth.clock.value }) }}
      </p>
      <ReauthField
        v-else-if="act.askPassword.value"
        :ref="(el) => (act.field.value = el as InstanceType<typeof ReauthField> | null)"
        v-model="act.password.value"
        :label="passwordLabel"
        :help="act.notice.value ? undefined : t('reauth.help')"
        :error="act.passwordError.value"
      />
      <component
        :is="DevPasswordHint"
        v-if="DevPasswordHint && act.askPassword.value"
        :server-id="serverId"
      />
      <slot name="after" />
    </template>
  </div>
</template>

<style scoped>
.admin {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.admin__acts {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.admin__wait {
  display: flex;
  justify-content: center;
  padding: var(--space-4) 0;
}

.admin__state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.admin__help {
  color: var(--tx-secondary);
}

.admin__alert {
  color: var(--crit);
}

.admin__notice {
  color: var(--warn);
}
</style>
