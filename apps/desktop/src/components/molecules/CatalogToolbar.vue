<script setup lang="ts">
import { type CatalogFilter, SEARCH_MAX } from "@/catalogue/sections";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import { t } from "@/i18n";

// La barre du Catalogue (HRT-241, BR-FICHE-063) : recherche, filtres avec leur nombre de cartes visibles, et « J'ai déjà
// un serveur de jeu ». Elle passe à la ligne en fenêtre étroite (la recherche au-dessus des filtres), jamais le texte d'un
// bouton (HRT-249). Échap vide la recherche. Le filtre « Bientôt » n'existe que s'il y a une fiche qui attend.
const props = defineProps<{
  counts: { all: number; ready: number; soon: number };
  showSoon: boolean;
  disabled?: boolean;
}>();

const query = defineModel<string>("query", { required: true });
const filter = defineModel<CatalogFilter>("filter", { required: true });
const emit = defineEmits<{ haveServer: [] }>();

function options(): { value: CatalogFilter; label: string }[] {
  const list: { value: CatalogFilter; label: string }[] = [
    { value: "all", label: t("catalogue.filterAll", { n: props.counts.all }) },
    { value: "ready", label: t("catalogue.filterReady", { n: props.counts.ready }) },
  ];
  if (props.showSoon)
    list.push({ value: "soon", label: t("catalogue.filterSoon", { n: props.counts.soon }) });
  return list;
}
</script>

<template>
  <div class="bar" data-catalogue-toolbar @keydown.esc="query = ''">
    <div class="bar__search">
      <HInput
        v-model="query"
        type="search"
        :label="t('catalogue.searchLabel')"
        :placeholder="t('catalogue.searchLabel')"
        :maxlength="SEARCH_MAX"
        hide-label
        :disabled="disabled"
        data-catalogue-search
      />
    </div>
    <HSegmented
      v-model="filter"
      :label="t('catalogue.filtersLabel')"
      :disabled="disabled"
      :options="options()"
      data-catalogue-filters
    />
    <HButton class="bar__have" variant="secondary" data-catalogue-have @click="emit('haveServer')">
      {{ t("catalogue.haveServer") }}
    </HButton>
  </div>
</template>

<style scoped>
.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.bar__search {
  flex: 1 1 var(--search-min);
  min-width: 0;
  max-width: var(--search-max);
}

.bar__have {
  margin-left: auto;
}
</style>
