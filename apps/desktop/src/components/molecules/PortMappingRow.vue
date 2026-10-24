<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import type { PortRow } from "@/deploy/draft";
import type { PortVerdict } from "@/deploy/messages";
import { t } from "@/i18n";
import type { DeployProtocol } from "@/link";

// Une correspondance de ports du formulaire de déploiement (HRT-69, BR-DEPLOY-007 à 009) : le port du serveur, le port
// du service, le protocole. Sous la rangée, UN message par priorité (`Callout inline`) : l'erreur de forme ou du serveur
// d'abord, puis l'avertissement. Un port refusé propose le plus petit port libre, que l'AGENT a calculé : la rangée ne
// recalcule aucun conflit. Les textes arrivent déjà en phrases (`errors`, `verdict`) : la rangée ne juge rien. Les
// libellés ne sont visibles que sur la première rangée.
const props = defineProps<{
  row: PortRow;
  hostError?: string;
  containerError?: string;
  /** Ce que le serveur dit de ce port (erreur, avertissement, port libre proposé). */
  verdict: PortVerdict;
  /** Le port est sous 1024 : avertissement, même sans réponse du serveur. */
  privileged: boolean;
  /** La réponse du serveur sur ce port n'est pas encore arrivée. */
  waiting: boolean;
  /** Première rangée : porte les libellés visibles. */
  first?: boolean;
  /** Rang (à partir de 1) pour le nom accessible du bouton « Retirer ». */
  position: number;
}>();

defineEmits<{
  "update:host": [value: string];
  "update:container": [value: string];
  "update:protocol": [value: DeployProtocol];
  suggest: [port: number];
  remove: [];
  "blur-host": [];
  "blur-container": [];
}>();

const PROTOCOLS = [
  { value: "tcp", label: "TCP" },
  { value: "udp", label: "UDP" },
] as const;

/** Aucune erreur sous la rangée : forme d'abord, puis le constat bloquant du serveur ; l'avertissement vient après. */
const clean = computed(
  () =>
    props.hostError === undefined &&
    props.containerError === undefined &&
    props.verdict.error === null,
);
</script>

<template>
  <div class="port" data-port-row :data-row-id="row.id">
    <div :class="['port__fields', { 'port__fields--labelled': first }]">
      <HInput
        :model-value="row.host"
        :label="t('deploy.hostPortLabel')"
        :hide-label="!first"
        :error="hostError ?? verdict.error ?? undefined"
        inputmode="numeric"
        :maxlength="5"
        mono
        @update:model-value="$emit('update:host', $event)"
        @blur="$emit('blur-host')"
      >
        <template v-if="waiting && !hostError" #suffix>
          <span class="port__wait" data-port-waiting>
            <HSpinner :label="t('deploy.portChecking')" />
          </span>
        </template>
      </HInput>
      <HInput
        :model-value="row.container"
        :label="t('deploy.containerPortLabel')"
        :hide-label="!first"
        :error="containerError"
        inputmode="numeric"
        :maxlength="5"
        mono
        @update:model-value="$emit('update:container', $event)"
        @blur="$emit('blur-container')"
      />
      <HSelect
        :model-value="row.protocol"
        :label="t('deploy.protocolLabel')"
        :hide-label="!first"
        :options="PROTOCOLS"
        @update:model-value="$emit('update:protocol', $event)"
      />
      <HButton
        class="port__remove"
        variant="ghost"
        size="sm"
        data-port-remove
        :aria-label="t('deploy.portRemove', { n: position })"
        @click="$emit('remove')"
      >
        <HIcon name="close" size="sm" />
      </HButton>
    </div>
    <div v-if="clean && (verdict.warning || privileged)" class="port__notes">
      <Callout v-if="verdict.warning" tone="warn" inline data-port-warning>
        {{ verdict.warning }}
        <HTag v-if="verdict.reserved" tone="warn">{{ t("deploy.portToConfirm") }}</HTag>
      </Callout>
      <Callout v-if="privileged" tone="warn" inline data-port-privileged>
        {{ t("deploy.portPrivileged") }}
      </Callout>
    </div>
    <div v-if="verdict.error && verdict.suggested !== null" class="port__notes">
      <Callout tone="info" inline data-port-suggested>
        {{ t("deploy.portSuggested", { port: verdict.suggested }) }}
        <template #actions>
          <HButton
            variant="secondary"
            size="sm"
            data-port-use
            @click="$emit('suggest', verdict.suggested as number)"
          >
            {{ t("deploy.portUseSuggested", { port: verdict.suggested }) }}
          </HButton>
        </template>
      </Callout>
    </div>
  </div>
</template>

<style scoped>
.port {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.port__fields {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto auto;
  align-items: start;
  gap: var(--space-3);
}

.port__fields--labelled .port__remove {
  margin-top: calc(var(--fs-small) * var(--lh-body) + var(--space-2));
}

.port__wait {
  display: flex;
  align-items: center;
  padding-inline-end: var(--space-3);
}

.port__notes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
</style>
