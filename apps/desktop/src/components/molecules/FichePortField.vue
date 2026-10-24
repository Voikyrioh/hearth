<script setup lang="ts">
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import Callout from "@/components/molecules/Callout.vue";
import PortNotes from "@/components/molecules/PortNotes.vue";
import type { PortVerdict } from "@/deploy/messages";
import { portRoleText } from "@/fiche/messages";
import { t } from "@/i18n";
import type { FichePort } from "@/link";

// Un port d'une fiche, à l'étape « Réponses » (HRT-102, BR-FICHE-019) : le port du serveur que les joueurs utiliseront,
// proposé par la fiche. Le conflit (qui tient quoi) est dit par l'AGENT (`verdict`), jamais recalculé ici : une erreur
// bloque, un avertissement non, et un port libre proche est proposé. Un port facultatif de la fiche se publie sur
// demande (case) ; tant qu'il n'est pas coché, aucun champ n'est montré. Les textes arrivent déjà en phrases.
const props = defineProps<{
  port: FichePort;
  modelValue: string;
  /** Le défaut de forme à montrer (déjà en phrase). */
  error?: string;
  verdict: PortVerdict;
  /** Le port saisi n'est pas celui que la fiche propose. */
  changed: boolean;
  /** La réponse du serveur sur ce port n'est pas encore arrivée. */
  waiting: boolean;
  /** Un port facultatif : la case de publication. */
  optional?: boolean;
  /** Le port facultatif est publié (sa case est cochée). */
  on?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  "update:on": [value: boolean];
  suggest: [port: number];
  blur: [];
}>();
</script>

<template>
  <div class="pfield" data-fiche-port :data-port="port.id">
    <HCheckbox
      v-if="optional"
      :model-value="on ?? false"
      :label="t('fiche.portOptional', { role: portRoleText(port.role) })"
      @update:model-value="emit('update:on', $event)"
    />
    <template v-if="!optional || on">
      <HInput
        :model-value="modelValue"
        :label="t('fiche.portLabel', { role: portRoleText(port.role) })"
        :help="t('fiche.portHelp')"
        :error="error ?? verdict.error ?? undefined"
        :placeholder="t('fiche.portPlaceholder')"
        inputmode="numeric"
        :maxlength="5"
        mono
        @update:model-value="emit('update:modelValue', $event)"
        @blur="emit('blur')"
      >
        <template v-if="waiting && !error" #suffix>
          <span class="pfield__wait" data-port-waiting>
            <HSpinner :label="t('deploy.portChecking')" />
          </span>
        </template>
      </HInput>
      <Callout v-if="changed && !error && !verdict.error" tone="info" inline data-port-changed>
        {{ t("fiche.portChanged") }}
      </Callout>
      <PortNotes
        :verdict="verdict"
        :privileged="false"
        :clean="!error && !verdict.error"
        @suggest="emit('suggest', $event)"
      />
      <p class="pfield__exposed" data-port-exposed>{{ t("deploy.portExposed") }}</p>
    </template>
  </div>
</template>

<style scoped>
.pfield {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.pfield__wait {
  display: inline-flex;
}

.pfield__exposed {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
