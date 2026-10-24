<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import EnvVarRow from "@/components/molecules/EnvVarRow.vue";
import { type EnvRow, valueShown } from "@/deploy/draft";
import type { EnvIssues } from "@/deploy/form";
import { envNameText, envValueText } from "@/deploy/messages";
import { MAX_ENV_VARS } from "@/deploy/rules";
import { t } from "@/i18n";

// La section « Variables d'environnement » du formulaire de déploiement (HRT-69, BR-DEPLOY-002, 006) : 0 à 100
// rangées, un compteur, l'avertissement permanent sur la visibilité des valeurs. Le nom et la valeur de chaque
// variable sont des secrets : cette section ne les écrit nulle part, elle les passe à la rangée. Les défauts de forme
// ne se montrent qu'une fois le champ quitté (`shown`). À 100 le bouton d'ajout est visible, inactif, avec « Limite
// atteinte » écrit à côté.
const props = defineProps<{
  rows: readonly EnvRow[];
  issues: readonly EnvIssues[];
  /** Le défaut de ce champ doit-il être montré ? (clé `env:{id}:name` ou `env:{id}:value`) */
  shown: (key: string) => boolean;
}>();

defineEmits<{
  add: [];
  remove: [id: number];
  update: [id: number, patch: Partial<Pick<EnvRow, "name" | "value" | "reveal">>];
  touch: [key: string];
}>();

function issueOf(id: number): EnvIssues | undefined {
  return props.issues.find((item) => item.id === id);
}

function nameError(row: EnvRow): string | undefined {
  const issue = issueOf(row.id)?.name;
  return issue && props.shown(`env:${row.id}:name`) ? envNameText(issue) : undefined;
}

/** Une valeur trop longue ou sur plusieurs lignes se dit tout de suite : elle ne se corrige qu'en retapant. */
function valueError(row: EnvRow): string | undefined {
  const issue = issueOf(row.id)?.value;
  return issue ? envValueText(issue) : undefined;
}
</script>

<template>
  <DashCard :title="t('deploy.envTitle')" data-env-section>
    <template #aside>
      <span class="count" data-env-count>{{ t("deploy.counter", { n: rows.length, max: MAX_ENV_VARS }) }}</span>
    </template>
    <Callout tone="info" data-env-visible>{{ t("deploy.envVisible") }}</Callout>
    <div v-if="rows.length > 0" class="rows">
      <EnvVarRow
        v-for="(row, index) in rows"
        :key="row.id"
        :row="row"
        :first="index === 0"
        :position="index + 1"
        :name-error="nameError(row)"
        :value-error="valueError(row)"
        @update:name="$emit('update', row.id, { name: $event })"
        @update:value="$emit('update', row.id, { value: $event })"
        @toggle="$emit('update', row.id, { reveal: !valueShown(row) })"
        @remove="$emit('remove', row.id)"
        @blur-name="$emit('touch', `env:${row.id}:name`)"
        @blur-value="$emit('touch', `env:${row.id}:value`)"
      />
    </div>
    <div class="add">
      <HButton
        variant="secondary"
        size="sm"
        data-env-add
        :disabled="rows.length >= MAX_ENV_VARS"
        :hint="t('deploy.limitReached')"
        @click="$emit('add')"
      >
        <HIcon name="plus" size="sm" />
        {{ t("deploy.envAdd") }}
      </HButton>
      <span v-if="rows.length >= MAX_ENV_VARS" class="limit" data-env-limit>
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
  gap: var(--space-3);
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
