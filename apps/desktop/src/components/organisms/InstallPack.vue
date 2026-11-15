<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import type { FicheInstallState } from "@/composables/useFicheInstall";
import {
  genreLabel,
  packFacts,
  packIssueText,
  packRefusalSentences,
  packSentences,
  readPack,
} from "@/fiche/pack";
import { t } from "@/i18n";

// Le pack de mods de l'installation (HRT-125, BR-MOD-006 à 009, 016) : la case « Installer avec un pack de mods », le choix
// du fichier, et ce que l'agent a LU dans le contenu (le genre n'est jamais un champ à remplir). Le moteur de serveur et la
// version de Minecraft se choisissent dans les réponses : ce composant dit seulement où les lire quand le pack ne les
// annonce pas. Un refus se dit en phrases, jamais avec un nom d'entrée de l'archive.
const props = defineProps<{ install: FicheInstallState }>();

const draft = computed(() => props.install.draft.value);
const pack = computed(() => readPack(draft.value?.pack ?? null));
const refusal = computed(() => {
  const check = draft.value?.pack?.check;
  return check?.kind === "refused" ? packRefusalSentences(check.refusal) : null;
});
const issue = computed(() => props.install.packProblem.value);
// Le défaut d'un pack lu (moteur, genre) se dit toujours ; les défauts « pas de fichier » et « pas lu » seulement une fois
// la page validée, pour ne pas crier avant que l'utilisateur ait pu choisir.
const issueText = computed(() => {
  const current = issue.value;
  if (current === null || current === "refused") return null;
  if ((current === "no_file" || current === "unread") && !props.install.shown("pack")) return null;
  return packIssueText(current);
});
</script>

<template>
  <DashCard v-if="install.packWay.value && draft" :title="t('pack.card')">
    <div class="pack" data-install-pack>
      <HCheckbox
        :model-value="draft.packWanted"
        :label="t('pack.toggle')"
        @update:model-value="install.setPackWanted($event)"
      />
      <template v-if="draft.packWanted">
        <p class="pack__help">{{ t("pack.help") }}</p>
        <div class="pack__choose">
          <HButton
            variant="secondary"
            :busy="install.packBusy.value"
            data-pack-choose
            @click="install.choosePack()"
          >
            {{ draft.pack ? t("pack.replace") : t("pack.choose") }}
          </HButton>
          <span v-if="draft.pack" class="pack__file" data-pack-file>
            {{ t("pack.chosen", { name: safeText(draft.pack.fileName) }) }}
          </span>
        </div>
        <span v-if="install.packBusy.value" class="pack__note" role="status">
          {{ t("pack.reading") }}
        </span>
        <Callout v-if="install.packUnavailable.value" tone="warn" role="alert" data-pack-unavailable>
          {{ t("pack.unavailable") }}
        </Callout>
        <Callout v-if="install.packReadFailed.value" tone="error" role="alert" data-pack-failed>
          {{ t("pack.readFailed") }}
        </Callout>
        <Callout v-if="refusal" tone="error" role="alert" data-pack-refusal>
          <p v-for="(sentence, index) in refusal" :key="index" class="pack__sentence">
            {{ sentence }}
          </p>
        </Callout>
        <div v-if="pack" class="pack__read" data-pack-read>
          <p class="pack__genre">
            <span class="pack__label">{{ t("pack.genreLabel") }}</span>
            <HTag tone="accent" data-pack-genre>{{ genreLabel(pack.kind) }}</HTag>
          </p>
          <ul class="pack__facts">
            <li v-for="fact in packFacts(pack)" :key="fact">{{ fact }}</li>
          </ul>
          <span class="pack__note">{{ t("pack.readNote") }}</span>
          <span v-if="pack.kind === 'server_pack'" class="pack__note" data-pack-engine-note>
            {{ t("pack.engineNote") }}
          </span>
          <span v-if="pack.kind === 'modrinth_pack'" class="pack__note">
            {{ t("pack.engineModrinth") }}
          </span>
          <span class="pack__note">{{ t("pack.memoryNote") }}</span>
          <ul class="pack__facts" data-pack-sentences>
            <li v-for="sentence in packSentences(pack)" :key="sentence">{{ sentence }}</li>
          </ul>
        </div>
        <Callout v-if="issueText" tone="warn" role="alert" data-pack-issue>
          {{ issueText }}
        </Callout>
      </template>
    </div>
  </DashCard>
</template>

<style scoped>
.pack {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.pack__help,
.pack__sentence {
  margin: 0;
}

.pack__choose {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.pack__file,
.pack__label {
  color: var(--tx-secondary);
}

.pack__genre {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
}

.pack__facts {
  margin: 0;
  padding-left: var(--space-4);
}

.pack__note {
  color: var(--tx-secondary);
}
</style>
