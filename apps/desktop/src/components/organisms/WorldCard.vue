<script setup lang="ts">
import { computed, useId } from "vue";
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import type { FicheInstallState } from "@/composables/useFicheInstall";
import {
  dateText,
  filesText,
  folderText,
  inspectionFiles,
  inspectionSize,
  notCopiedLines,
  notEnoughRoom,
  refusalText,
  sameWorld,
  sizeText,
  worldChoice,
  worldName,
  worldRows,
} from "@/fiche/world";
import { t } from "@/i18n";
import { formatCount, formatSize } from "@/serviceOps/messages";

// La carte « Monde » de l'installation d'un serveur de jeu (HRT-106, BR-MONDE-002, 003, 006, 009, 012, 014) : « Nouveau
// monde » (par défaut) ou « Reprendre un monde existant ». La carte n'existe que si la fiche sait reprendre un monde et si
// l'agent a répondu à la lecture ; elle n'est ni gardée ni journalisée. Le choix est DÉSACTIVÉ, avec sa raison, quand aucun
// dossier du serveur n'est ouvert aux services. La liste dit le dossier de chaque monde ; un monde dont le nom est refusé
// par le jeu est listé mais ne se choisit pas. Une fois choisi, l'inspection dit ce que la copie écrira et ce qu'elle
// laissera où il est ; la copie ne supprime ni ne modifie jamais le monde d'origine, et si un jeu écrit dans ce monde
// pendant la copie, elle peut être incohérente : le client le dit toujours. Le monde porte son nom et ses réglages figés,
// la page ne les pose pas. Le geste « Je comprends » sur la version du jeu est une case du récapitulatif.
const props = defineProps<{ install: FicheInstallState }>();

const { server } = useCurrentServer();
const draft = computed(() => props.install.draft.value);
const worlds = computed(() => props.install.worldImport.worlds.value);
const inspection = computed(() => props.install.worldImport.inspection.value);
const name = useId();

const view = computed(() => (worlds.value.kind === "ready" ? worlds.value.view : null));
const choice = computed(() => (view.value ? worldChoice(view.value) : null));
const rows = computed(() => (view.value ? worldRows(view.value) : []));
const resume = computed(() => draft.value?.world.resume === true);
const chosen = computed(() => {
  const world = draft.value?.world;
  if (!world || world.folder === null || world.path === null) return null;
  return rows.value.find((row) => sameWorld(row, world)) ?? null;
});
const left = computed(() =>
  inspection.value.kind === "ready" ? notCopiedLines(inspection.value.inspection) : [],
);
const room = computed(
  () => inspection.value.kind === "ready" && notEnoughRoom(inspection.value.inspection),
);
const maxEntries = computed(() => formatCount(view.value?.maxEntries ?? 0));
</script>

<template>
  <DashCard v-if="draft && view" :title="t('fiche.world.title')">
    <div class="world" data-world-card>
      <fieldset class="world__set">
        <legend class="world__legend">{{ t("fiche.world.legend") }}</legend>
        <label :class="['world__card', { 'world__card--on': !resume }]" data-world-new>
          <input
            class="world__radio"
            type="radio"
            :name="name"
            :checked="!resume"
            @change="install.setResume(false)"
          />
          <span class="world__text">
            <span class="world__name">{{ t("fiche.world.new") }}</span>
            <span class="world__help">{{ t("fiche.world.newHelp") }}</span>
          </span>
          <HIcon v-if="!resume" name="check" size="sm" />
        </label>
        <label
          :class="[
            'world__card',
            { 'world__card--on': resume, 'world__card--off': choice?.kind === 'no_folder' },
          ]"
          data-world-existing
        >
          <input
            class="world__radio"
            type="radio"
            :name="name"
            :checked="resume"
            :disabled="choice?.kind === 'no_folder'"
            @change="install.setResume(true)"
          />
          <span class="world__text">
            <span class="world__name">{{ t("fiche.world.existing") }}</span>
            <span class="world__help">{{
              choice?.kind === "no_folder" ? t("fiche.world.noFolder") : t("fiche.world.existingHelp")
            }}</span>
          </span>
          <HIcon v-if="resume" name="check" size="sm" />
        </label>
      </fieldset>
      <RouterLink
        v-if="server"
        class="world__link"
        :to="{ name: 'existing-server-help', params: { id: server.id } }"
        data-world-help
      >
        {{ t("fiche.help.link") }}
      </RouterLink>

      <template v-if="resume">
        <!-- Le monde choisi : la carte « Monde repris » et ce que la copie fera -->
        <div v-if="chosen" class="world__chosen" data-world-chosen>
          <div class="world__row">
            <span class="world__strong" data-world-name>{{ worldName(chosen.entry) }}</span>
            <HButton variant="ghost" size="sm" data-world-change @click="install.changeWorld()">
              {{ t("fiche.world.change") }}
            </HButton>
          </div>
          <span class="world__help">{{ t("fiche.world.sumFolder", { folder: folderText(chosen.folder) }) }}</span>
          <p v-if="inspection.kind === 'loading'" class="world__help" role="status" data-world-measuring>
            {{ t("fiche.world.measuring") }}
          </p>
          <p v-else-if="inspection.kind === 'ready'" class="world__help" data-world-measure>
            {{
              t("fiche.world.copyOf", {
                size: inspectionSize(inspection.inspection),
                files: inspectionFiles(inspection.inspection),
              })
            }}
          </p>
          <Callout v-if="inspection.kind === 'refused'" tone="error" role="alert" inline data-world-refused>
            {{ refusalText(inspection.reason) }}
          </Callout>
          <Callout v-else-if="inspection.kind === 'failed'" tone="error" role="alert" inline data-world-failed>
            {{ t("fiche.world.inspectFailed") }}
          </Callout>
          <Callout v-if="room && inspection.kind === 'ready'" tone="error" role="alert" inline data-world-room>
            {{
              inspection.inspection.freeBytes === null
                ? t("fiche.world.noRoomBare")
                : t("fiche.world.noRoom", {
                    needed: formatSize(inspection.inspection.bytes),
                    free: formatSize(inspection.inspection.freeBytes),
                  })
            }}
          </Callout>
          <Callout v-for="line in left" :key="line" tone="info" inline data-world-left>{{ line }}</Callout>
          <Callout tone="info" inline data-world-never-deleted>{{ t("fiche.world.neverDeleted") }}</Callout>
          <Callout tone="warn" inline data-world-coherence>{{ t("fiche.world.coherence") }}</Callout>
          <HCheckbox
            :model-value="draft.world.keepIfChanged"
            :label="t('fiche.world.keepIfChanged')"
            data-world-keep
            @update:model-value="install.setKeepIfChanged($event)"
          />
          <p class="world__help">{{ t("fiche.world.carried") }}</p>
          <p class="world__help">{{ t("fiche.world.otherPort") }}</p>
        </div>

        <!-- La liste : un monde par ligne, avec son dossier -->
        <div v-else class="world__picker" data-world-picker>
          <h3 class="world__pick-title">{{ t("fiche.world.pick") }}</h3>
          <p v-if="rows.length === 0" class="world__empty" data-world-empty>{{ t("fiche.world.empty") }}</p>
          <ul v-else class="world__list" :aria-label="t('fiche.world.pick')">
            <li v-for="row in rows" :key="`${row.folder}/${row.entry.path}`" class="world__item" data-world-row>
              <span class="world__text">
                <span class="world__name">{{ worldName(row.entry) }}</span>
                <span class="world__help">
                  {{ folderText(row.folder) }} · {{ filesText(row.entry.files) }} ·
                  {{ sizeText(row.entry.bytes) }} · {{ dateText(row.entry.modifiedAt) }}
                </span>
                <span v-if="!row.entry.usable" class="world__help" data-world-unusable>
                  {{ t("fiche.world.unusable") }}
                </span>
              </span>
              <HButton
                variant="secondary"
                size="sm"
                :disabled="!row.entry.usable"
                :aria-label="t('fiche.world.chooseLabel', { name: worldName(row.entry) })"
                data-world-choose
                @click="install.chooseWorld(row.folder, row.entry.path, row.entry.name)"
              >
                {{ t("fiche.world.choose") }}
              </HButton>
            </li>
          </ul>
          <Callout v-if="choice?.kind === 'folders' && choice.truncated" tone="warn" inline data-world-truncated>
            {{ t("fiche.world.truncated", { n: maxEntries }) }}
          </Callout>
          <Callout v-if="choice?.kind === 'folders' && choice.unreadable > 0" tone="warn" inline data-world-unreadable>
            {{ t("fiche.world.unreadable") }}
          </Callout>
          <p v-if="install.shown('world') && install.worldProblem.value" class="world__error" role="alert" data-world-missing>
            {{ t("fiche.world.missing") }}
          </p>
        </div>
      </template>
      <HSkeleton v-else-if="worlds.kind === 'loading'" />
    </div>
  </DashCard>
</template>

<style scoped>
.world {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.world__set {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(0, 1fr));
  gap: var(--space-3);
  min-width: 0;
  padding: 0;
  margin: 0;
  border: 0;
}

.world__legend {
  margin-bottom: var(--space-2);
  padding: 0;
  font-weight: var(--fw-medium);
}

.world__card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  cursor: pointer;
}

.world__card--on {
  border-color: var(--ac);
}

.world__card--off {
  cursor: not-allowed;
  opacity: 0.7;
}

.world__card:focus-within {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.world__radio {
  position: absolute;
  opacity: 0;
  pointer-events: none;
}

.world__text {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.world__name,
.world__strong {
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.world__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.world__link {
  align-self: flex-start;
  color: var(--ac);
  text-decoration: underline;
}

.world__chosen,
.world__picker {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.world__row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.world__pick-title {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.world__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.world__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
}

.world__empty {
  margin: 0;
  color: var(--tx-secondary);
}

.world__error {
  margin: 0;
  color: var(--crit);
  font-size: var(--fs-small);
}
</style>
