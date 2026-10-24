<script setup lang="ts">
import { computed, ref } from "vue";
import HInput from "@/components/atoms/HInput.vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import Callout from "@/components/molecules/Callout.vue";
import GameSettingRow from "@/components/molecules/GameSettingRow.vue";
import { t } from "@/i18n";
import type { FicheTheme } from "@/link";
import { type Edits, resettable } from "@/settings/edits";
import {
  type Filter,
  groupByTheme,
  isCommon,
  opensByDefault,
  type Scope,
  shown,
} from "@/settings/filter";
import type { SettingModel } from "@/settings/model";
import SettingThemeCard from "./SettingThemeCard.vue";

// La liste des réglages d'un jeu (HRT-109, BR-FICHE-041, 043, 045), la même dans l'onglet « Réglages » et dans « Voir tous
// les réglages » de l'installation : en tête la recherche (par libellé, nom technique et phrase, sur tous les thèmes et
// tous les niveaux), le filtre « Modifiés » et la bascule « Courants / Tout afficher » ; puis une carte repliable par
// thème de la fiche, dans l'ordre de la fiche. Un thème qui contient un résultat s'ouvre. Les réglages que le jeu n'a
// plus sont rangés à part. Présentation seule : l'état (valeurs, changements) est à l'appelant.
const props = defineProps<{
  models: readonly SettingModel[];
  themes: readonly FicheTheme[];
  edits: Edits;
  mode: "service" | "install";
  /** Le défaut de forme de chaque valeur saisie, déjà en phrase. */
  issues: ReadonlyMap<string, string>;
  filter: Filter;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  "update:filter": [filter: Filter];
  set: [id: string, value: string];
  unset: [id: string];
  reset: [id: string];
  resetTheme: [themeId: string];
  blur: [id: string];
  link: [id: string];
}>();

const search = ref<InstanceType<typeof HInput> | null>(null);

const active = computed(() => props.models.filter((model) => model.lock !== "gone"));
const gone = computed(() => props.models.filter((model) => model.lock === "gone"));
const visible = computed(() => shown(active.value, props.filter, props.edits));
const groups = computed(() => groupByTheme(props.themes, active.value, visible.value, props.edits));
const searching = computed(() => props.filter.query.trim() !== "");
const hiddenAdvanced = computed(() =>
  searching.value || props.filter.scope === "all"
    ? 0
    : active.value.filter((model) => !isCommon(model)).length,
);

const filterOptions = computed(() => [
  { value: "all" as const, label: t("gameSettings.filterAll") },
  { value: "modified" as const, label: t("gameSettings.filterModified") },
]);
const scopeOptions = computed(() => [
  { value: "common" as const, label: t("gameSettings.scopeCommon") },
  { value: "all" as const, label: t("gameSettings.scopeAll") },
]);

function patch(next: Partial<Filter>) {
  emit("update:filter", { ...props.filter, ...next });
}

function themeResettable(models: readonly SettingModel[]): boolean {
  return models.some((model) => resettable(model, props.edits[model.id]));
}

const found = computed(() => visible.value.length);

defineExpose({
  /** Met le curseur dans le champ de recherche (la touche « / »). */
  focusSearch(): void {
    const root = (search.value as unknown as { $el?: HTMLElement } | null)?.$el;
    root?.querySelector<HTMLElement>("input")?.focus();
  },
});
</script>

<template>
  <div class="list" data-settings-list>
    <div class="list__bar">
      <HInput
        ref="search"
        class="list__search"
        type="search"
        :model-value="filter.query"
        :label="t('gameSettings.search')"
        :placeholder="t('gameSettings.search')"
        data-settings-search
        @update:model-value="patch({ query: $event })"
      />
      <HSegmented
        :model-value="filter.modifiedOnly ? 'modified' : 'all'"
        :options="filterOptions"
        :label="t('gameSettings.filterLabel')"
        data-settings-filter
        @update:model-value="patch({ modifiedOnly: $event === 'modified' })"
      />
      <HSegmented
        :model-value="filter.scope"
        :options="scopeOptions"
        :label="t('gameSettings.scopeLabel')"
        data-settings-scope
        @update:model-value="patch({ scope: $event as Scope })"
      />
    </div>
    <p class="sr-only" role="status" data-settings-found>
      {{ searching || filter.modifiedOnly ? t("gameSettings.found", { n: found }) : "" }}
    </p>

    <Callout v-if="hiddenAdvanced > 0" tone="info" inline data-settings-hidden>
      {{ t("gameSettings.hiddenAdvanced", { n: hiddenAdvanced }) }}
    </Callout>

    <p v-if="groups.length === 0" class="list__none" data-settings-none>
      {{ t("gameSettings.none") }}
    </p>

    <SettingThemeCard
      v-for="group in groups"
      :key="group.theme.id"
      :label="group.theme.label"
      :total="group.total"
      :modified="group.modified"
      :open="opensByDefault(group)"
      :force-open="searching"
      :can-reset="themeResettable(group.models)"
      :disabled="disabled"
      :data-theme="group.theme.id"
      @reset="emit('resetTheme', group.theme.id)"
    >
      <GameSettingRow
        v-for="model in group.models"
        :key="model.id"
        :model="model"
        :edit="edits[model.id]"
        :mode="mode"
        :issue="issues.get(model.id)"
        :disabled="disabled"
        @set="emit('set', model.id, $event)"
        @unset="emit('unset', model.id)"
        @reset="emit('reset', model.id)"
        @blur="emit('blur', model.id)"
        @link="emit('link', model.id)"
      />
    </SettingThemeCard>

    <SettingThemeCard
      v-if="gone.length > 0"
      :label="t('gameSettings.goneTitle')"
      :total="gone.length"
      :modified="0"
      :open="false"
      data-settings-gone
    >
      <GameSettingRow
        v-for="model in gone"
        :key="model.id"
        :model="model"
        :edit="undefined"
        :mode="mode"
        disabled
      />
    </SettingThemeCard>

    <slot name="after" />
  </div>
</template>

<style scoped>
.list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.list__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-3) var(--space-4);
}

.list__search {
  flex: 1 1 var(--audit-search-min);
  min-width: 0;
}

.list__none {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
