<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import PortMappingRow from "@/components/molecules/PortMappingRow.vue";
import type { PortRow } from "@/deploy/draft";
import type { PortIssues } from "@/deploy/form";
import { type PortVerdict, portText } from "@/deploy/messages";
import { MAX_PORT_MAPPINGS, PRIVILEGED_PORT_BELOW, parsePort } from "@/deploy/rules";
import { t } from "@/i18n";
import type { DeployProtocol } from "@/link";

// La section « Ports » du formulaire de déploiement (HRT-69, BR-DEPLOY-007 à 009, 031) : 0 à 50 correspondances, un
// compteur, et dès qu'une rangée existe, l'avertissement permanent « accessible à tout appareil qui peut joindre
// votre serveur ». Les constats de conflit (qui tient quoi, quel port libre proposer) viennent de l'agent et arrivent
// dans `verdicts` : la section n'en recalcule aucun (BR-DEPLOY-032).
const props = defineProps<{
  rows: readonly PortRow[];
  issues: readonly PortIssues[];
  verdicts: ReadonlyMap<number, PortVerdict>;
  /** Le serveur n'a pas encore répondu sur les ports. */
  waiting: boolean;
  shown: (key: string) => boolean;
}>();

defineEmits<{
  add: [];
  remove: [id: number];
  update: [id: number, patch: Partial<Pick<PortRow, "host" | "container">>];
  protocol: [id: number, protocol: DeployProtocol];
  suggest: [id: number, port: number];
  touch: [key: string];
}>();

const NO_VERDICT: PortVerdict = { error: null, warning: null, reserved: false, suggested: null };

function issueOf(id: number): PortIssues | undefined {
  return props.issues.find((item) => item.id === id);
}

function hostError(row: PortRow): string | undefined {
  const issue = issueOf(row.id)?.host;
  if (!issue || !props.shown(`port:${row.id}:host`)) return undefined;
  return issue === "duplicate" ? t("deploy.portDuplicate") : portText(issue, "host");
}

function containerError(row: PortRow): string | undefined {
  const issue = issueOf(row.id)?.container;
  return issue && props.shown(`port:${row.id}:container`)
    ? portText(issue, "container")
    : undefined;
}

/** Sous 1024 : avertissement tout de suite, sans attendre le serveur (BR-DEPLOY-007). */
function privileged(row: PortRow): boolean {
  const parsed = parsePort(row.host);
  return "port" in parsed && parsed.port < PRIVILEGED_PORT_BELOW;
}

/** Le constat du serveur n'a de sens que pour un port bien formé et seul de son espèce. */
function verdictOf(row: PortRow): PortVerdict {
  return issueOf(row.id)?.host ? NO_VERDICT : (props.verdicts.get(row.id) ?? NO_VERDICT);
}

function waitingFor(row: PortRow): boolean {
  return props.waiting && !issueOf(row.id)?.host && !props.verdicts.get(row.id)?.error;
}
</script>

<template>
  <DashCard :title="t('deploy.portsTitle')" data-ports-section>
    <template #aside>
      <span class="count" data-ports-count>
        {{ t("deploy.counter", { n: rows.length, max: MAX_PORT_MAPPINGS }) }}
      </span>
    </template>
    <div v-if="rows.length > 0" class="rows">
      <PortMappingRow
        v-for="(row, index) in rows"
        :key="row.id"
        :row="row"
        :first="index === 0"
        :position="index + 1"
        :host-error="hostError(row)"
        :container-error="containerError(row)"
        :verdict="verdictOf(row)"
        :privileged="privileged(row) && !hostError(row)"
        :waiting="waitingFor(row)"
        @update:host="$emit('update', row.id, { host: $event })"
        @update:container="$emit('update', row.id, { container: $event })"
        @update:protocol="$emit('protocol', row.id, $event)"
        @suggest="$emit('suggest', row.id, $event)"
        @remove="$emit('remove', row.id)"
        @blur-host="$emit('touch', `port:${row.id}:host`)"
        @blur-container="$emit('touch', `port:${row.id}:container`)"
      />
      <Callout tone="info" data-ports-exposed>{{ t("deploy.portExposed") }}</Callout>
    </div>
    <div class="add">
      <HButton
        variant="secondary"
        size="sm"
        data-port-add
        :disabled="rows.length >= MAX_PORT_MAPPINGS"
        :hint="t('deploy.limitReached')"
        @click="$emit('add')"
      >
        <HIcon name="plus" size="sm" />
        {{ t("deploy.portAdd") }}
      </HButton>
      <span v-if="rows.length >= MAX_PORT_MAPPINGS" class="limit" data-ports-limit>
        {{ t("deploy.limitReached") }}
      </span>
    </div>
  </DashCard>
</template>

<style scoped>
.count {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
}

.rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.add {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.limit {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
