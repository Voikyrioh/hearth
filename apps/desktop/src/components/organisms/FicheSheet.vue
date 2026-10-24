<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import ProofTag from "@/components/molecules/ProofTag.vue";
import FicheResourcesCard from "@/components/organisms/FicheResourcesCard.vue";
import { noFigureAtAll } from "@/fiche/fit";
import { registryOf } from "@/fiche/image";
import {
  boundsText,
  diskNeededText,
  downloadText,
  durationText,
  folderRoleText,
  kindText,
  portPublishText,
  portRoleText,
  registryText,
  stopTexts,
  valueLabel,
} from "@/fiche/messages";
import { proofOf, proofUrl, proven } from "@/fiche/proof";
import { t } from "@/i18n";
import type { FicheDetail } from "@/link";
import type { EngineCapacity } from "@/resources";

// Les cartes de lecture de la fiche d'un jeu (HRT-102, BR-FICHE-004, 008, 010) : Image, Ports, Dossiers, Ressources,
// Espace disque, Téléchargements, Arrêt du jeu, Questions, Ce que cette fiche impose et, quand il y en a un, Contrat de
// licence. Tout est LU dans la fiche que l'agent propose : libellés, chiffres, preuves, aucun repère inventé. Une fiche
// ne demande jamais un compte : il n'y a aucune ligne « compte exigé » (BR-FICHE-011). Un chiffre sans preuve n'a pas de
// nombre. Les adresses (contrat, source d'un chiffre) ne s'ouvrent pas d'ici : la page demande à la coquille.
const props = defineProps<{
  detail: FicheDetail;
  capacity: EngineCapacity | null;
}>();

const emit = defineEmits<{
  source: [resource: "cpu" | "memory" | "disk"];
  agreement: [id: string];
  imposed: [];
}>();

const image = computed(() => props.detail.image);
const registry = computed(() => registryText(registryOf(image.value.repository)));
const firstStart = computed(() => props.detail.firstStart);
const disk = computed(() => props.detail.resources.disk);
/** Aucun besoin connu : la carte Ressources le dit une fois, la carte Espace disque ne le redit pas. */
const nothingKnown = computed(() => noFigureAtAll(props.detail));
const diskNeed = computed(() => proven(disk.value.installBytes, disk.value.evidence));
const stop = computed(() => (props.detail.stop ? stopTexts(props.detail.stop) : []));
const downloads = computed(() => firstStart.value?.downloads ?? []);
</script>

<template>
  <div class="sheet" data-fiche-sheet>
    <DashCard :title="t('fiche.cardImage')" data-fiche-image>
      <p class="sheet__mono" data-image-ref>{{ safeText(image.repository) }}:{{ safeText(image.tag) }}</p>
      <template v-if="image.digest !== null">
        <p class="sheet__line">{{ t("fiche.imageDigest") }}</p>
        <p class="sheet__mono" data-image-digest>{{ safeText(image.digest) }}</p>
      </template>
      <p v-else class="sheet__line" data-image-digest>{{ t("fiche.imageNoDigest") }}</p>
      <p class="sheet__line">{{ registry }}</p>
      <p class="sheet__line">{{ t("fiche.imageVersion", { version: safeText(image.tag) }) }}</p>
    </DashCard>

    <DashCard :title="t('fiche.cardPorts')" data-fiche-ports>
      <div class="sheet__scroll">
        <table class="sheet__table" :aria-label="t('fiche.portsHead')">
          <thead>
            <tr>
              <th scope="col">{{ t("fiche.colPort") }}</th>
              <th scope="col">{{ t("fiche.colProtocol") }}</th>
              <th scope="col">{{ t("fiche.colRole") }}</th>
              <th scope="col">{{ t("fiche.colPublish") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="port in detail.ports" :key="port.id" data-fiche-port-row>
              <td class="sheet__mono">{{ port.defaultHost ?? port.container }}</td>
              <td>{{ port.protocol.toUpperCase() }}</td>
              <td>{{ portRoleText(port.role) }}</td>
              <td>{{ portPublishText(port.publish) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </DashCard>

    <DashCard :title="t('fiche.cardFolders')" data-fiche-folders>
      <ul class="sheet__list">
        <li v-for="folder in detail.folders" :key="folder.id" data-fiche-folder>
          <span class="sheet__strong">{{ folderRoleText(folder.role) }}</span>
          <span class="sheet__mono">{{ safeText(folder.target) }}</span>
          <span class="sheet__muted">{{ folder.backup ? t("fiche.folderBackup") : t("fiche.folderNoBackup") }}</span>
        </li>
      </ul>
      <p class="sheet__muted">{{ t("fiche.foldersNote") }}</p>
    </DashCard>

    <FicheResourcesCard :detail="detail" :capacity="capacity" @source="emit('source', $event)" />

    <DashCard v-if="!nothingKnown" :title="t('fiche.cardDisk')" data-fiche-disk>
      <p class="sheet__line" data-disk-needed>{{ diskNeededText(diskNeed) }}</p>
      <ProofTag
        :proof="proofOf(disk.evidence)"
        :source="proofUrl(disk.evidence) !== null"
        :subject="t('fiche.resDisk')"
        @source="emit('source', 'disk')"
      />
    </DashCard>

    <DashCard :title="t('fiche.cardDownloads')" data-fiche-downloads>
      <template v-if="downloads.length > 0">
        <div v-for="(download, index) in downloads" :key="index" class="sheet__block" data-fiche-download>
          <p class="sheet__line">{{ downloadText(download) }}</p>
          <ProofTag :proof="proofOf(download.evidence)" :subject="safeText(download.what)" />
        </div>
      </template>
      <p v-else class="sheet__line" data-fiche-download-none>{{ t("fiche.downloadNone") }}</p>
      <template v-if="firstStart">
        <p class="sheet__line" data-first-start-long>
          {{ t("fiche.firstStartLong", { duration: durationText(firstStart.readyTimeoutSeconds) }) }}
        </p>
        <p v-if="firstStart.worldGeneration" class="sheet__muted">{{ t("fiche.firstStartWorld") }}</p>
        <p v-if="firstStart.stages.length > 0" class="sheet__muted" data-first-start-stages>
          {{ t("fiche.firstStartStages", { list: firstStart.stages.map((stage) => safeText(stage)).join(", ") }) }}
        </p>
      </template>
    </DashCard>

    <DashCard v-if="stop.length > 0" :title="t('fiche.cardStop')" data-fiche-stop>
      <p v-for="line in stop" :key="line" class="sheet__line">{{ line }}</p>
    </DashCard>

    <DashCard :title="t('fiche.cardQuestions')" data-fiche-questions>
      <!-- Tous les réglages du jeu et leurs thèmes (HRT-109, BR-FICHE-037, 041) -->
      <template v-if="detail.settings.length > 0">
        <p class="sheet__line" data-fiche-settings-count>
          {{ t("gameSettings.sheetCount", { n: detail.settings.length, m: detail.questions.length }) }}
        </p>
        <p v-if="detail.themes.length > 0" class="sheet__muted" data-fiche-settings-themes>
          {{ t("gameSettings.sheetThemes", { list: detail.themes.map((theme) => safeText(theme.label)).join(", ") }) }}
        </p>
      </template>
      <p v-if="detail.questions.length === 0" class="sheet__line">{{ t("fiche.questionsNone") }}</p>
      <template v-else>
        <p class="sheet__muted">{{ t("fiche.questionsCount", { n: detail.questions.length }) }}</p>
        <ul class="sheet__list">
          <li v-for="question in detail.questions" :key="question.id" data-fiche-question>
            <span class="sheet__strong">{{ safeText(question.label) }}</span>
            <span class="sheet__muted">{{ kindText(question.kind) }}</span>
            <span class="sheet__line">
              <template v-if="question.proposed !== null">
                {{ t("fiche.proposedValue", { value: valueLabel(question, question.proposed) }) }}
              </template>
              <template v-else>{{ t("fiche.noProposedValue") }}</template>
            </span>
            <span v-if="boundsText(question)" class="sheet__muted">{{ boundsText(question) }}</span>
            <span v-if="question.choices.length > 0 && question.kind === 'choice'" class="sheet__muted">
              {{ t("fiche.choicesList", { list: question.choices.map((choice) => safeText(choice.label)).join(", ") }) }}
            </span>
            <span v-if="question.frozenAfterCreation" class="sheet__frozen">
              <HIcon name="lock" size="sm" />
              {{ t("fiche.frozen") }}
            </span>
          </li>
        </ul>
      </template>
    </DashCard>

    <DashCard :title="t('fiche.cardImposed')" data-fiche-imposed>
      <p class="sheet__line" data-imposed-count>
        {{ detail.imposed.length === 0 ? t("fiche.imposedNone") : t("fiche.imposedCount", { n: detail.imposed.length }) }}
      </p>
      <HButton
        v-if="detail.imposed.length > 0"
        class="sheet__action"
        variant="secondary"
        size="sm"
        data-imposed-open
        @click="emit('imposed')"
      >
        {{ t("fiche.imposedSee") }}
      </HButton>
    </DashCard>

    <DashCard v-if="detail.agreements.length > 0" :title="t('fiche.cardAgreements')" data-fiche-agreements>
      <div v-for="agreement in detail.agreements" :key="agreement.id" class="sheet__block" data-fiche-agreement>
        <p class="sheet__strong">{{ safeText(agreement.title) }}</p>
        <HButton variant="secondary" size="sm" data-agreement-read @click="emit('agreement', agreement.id)">
          <HIcon name="external" size="sm" />
          {{ t("fiche.agreementRead") }}
        </HButton>
      </div>
    </DashCard>
  </div>
</template>

<style scoped>
.sheet {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  min-width: 0;
}

.sheet__line {
  margin: 0;
  overflow-wrap: anywhere;
}

.sheet__muted {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.sheet__strong {
  font-weight: var(--fw-semibold);
}

.sheet__mono {
  margin: 0;
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}

.sheet__block {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.sheet__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: 0;
  margin: 0;
  list-style: none;
}

.sheet__list li {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.sheet__frozen {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--warn);
  font-size: var(--fs-small);
}

.sheet__scroll {
  overflow-x: auto;
}

.sheet__table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.sheet__table th,
.sheet__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
}

.sheet__table thead th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
}

.sheet__action {
  align-self: flex-start;
}
</style>
