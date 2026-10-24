<script setup lang="ts">
import { computed, ref } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import type { FicheInstallState } from "@/composables/useFicheInstall";
import { changedExtras } from "@/fiche/answers";
import { t } from "@/i18n";
import type { Edits } from "@/settings/edits";
import type { Filter } from "@/settings/filter";
import { installModelsOf } from "@/settings/model";
import GameSettingsList from "./GameSettingsList.vue";

// « Voir tous les réglages » dans l'installation (HRT-109, BR-FICHE-040) : sous la carte des questions (huit au plus), un
// bouton déplie, DANS la page, la même liste que l'onglet « Réglages » (mêmes lignes, mêmes thèmes, même recherche) sur les
// valeurs proposées par la fiche. Pas de pied collant, pas de fichier brut. Un réglage touché est compté dans le
// récapitulatif (« {n} réglage(s) différent(s) des valeurs proposées »). Les réglages essentiels sont déjà posés plus
// haut : ils ne sont pas répétés ici. L'import d'un texte collé a besoin d'un service installé (c'est l'agent qui lit
// le texte, avec le format du jeu) : le bouton est présent, inactif, avec sa phrase. Les valeurs saisies, mots de passe
// compris, sont celles du brouillon de l'installation : mémoire seulement (BR-FICHE-034).
const props = defineProps<{ install: FicheInstallState }>();

const open = ref(false);
const filter = ref<Filter>({ query: "", modifiedOnly: false, scope: "all" });

const detail = computed(() => props.install.detail.value);
const draft = computed(() => props.install.draft.value);

const models = computed(() =>
  detail.value && draft.value
    ? installModelsOf(props.install.extras.value, draft.value.variant)
    : [],
);
const themes = computed(() => detail.value?.themes ?? []);
/** Les réponses touchées, vues comme les changements de l'onglet (poser une valeur). */
const edits = computed<Edits>(() => {
  const out: Edits = {};
  for (const [id, value] of Object.entries(draft.value?.answers ?? {})) {
    if (props.install.extras.value.some((setting) => setting.question.id === id)) {
      out[id] = { action: "set", value };
    }
  }
  return out;
});
const changed = computed(() =>
  draft.value ? changedExtras(props.install.extras.value, draft.value.answers).length : 0,
);
const asked = computed(() => props.install.questions.value.length);
// Les réglages que cette page règle : les questions plus les autres ; ceux que la fiche impose sont dans sa propre carte.
const total = computed(() => props.install.extras.value.length + asked.value);
const visibleIssues = computed(() => {
  const found = new Map<string, string>();
  for (const [id, text] of props.install.extraProblems.value) {
    if (props.install.shown(`x:${id}`)) found.set(id, text);
  }
  return found;
});
const available = computed(
  () => (detail.value?.settings.length ?? 0) > 0 && models.value.length > 0,
);
</script>

<template>
  <div v-if="available" class="more" data-install-settings>
    <div class="more__bar">
      <HButton
        variant="ghost"
        :aria-expanded="open"
        data-install-settings-toggle
        @click="open = !open"
      >
        {{ open ? t("gameSettings.installHide") : t("gameSettings.installShow") }}
      </HButton>
      <span v-if="changed > 0" class="more__changed" role="status" data-install-settings-changed>
        {{ t("gameSettings.installChanged", { n: changed }) }}
      </span>
    </div>

    <div v-if="open" class="more__body" data-install-settings-body>
      <p class="more__intro">{{ t("gameSettings.installIntro", { n: total, m: asked }) }}</p>
      <GameSettingsList
        v-model:filter="filter"
        :models="models"
        :themes="themes"
        :edits="edits"
        mode="install"
        :issues="visibleIssues"
        @set="(id, value) => install.setAnswer(id, value)"
        @reset="(id) => install.resetAnswer(id)"
        @blur="(id) => install.touch(`x:${id}`)"
      />
      <div class="more__import">
        <HButton variant="secondary" size="sm" disabled data-install-settings-import>
          {{ t("gameSettings.import") }}
        </HButton>
        <p class="more__soon">{{ t("gameSettings.installImport") }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.more {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.more__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.more__changed {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.more__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.more__intro {
  margin: 0;
  color: var(--tx-secondary);
}

.more__import {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
}

.more__soon {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
