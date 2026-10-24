<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import StepTrail from "@/components/molecules/StepTrail.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import AgreementDialog from "@/components/organisms/AgreementDialog.vue";
import { useFicheLinks } from "@/composables/useFicheLinks";
import type { ActReport } from "@/composables/useReauth";
import { portVerdict, refusalText } from "@/deploy/messages";
import { changedAnswers, currentAnswer } from "@/fiche/answers";
import { type AttentionPoint, acceptOf, allValidated, attentionPoints } from "@/fiche/attention";
import type { FicheDraft } from "@/fiche/draft";
import { registryOf } from "@/fiche/image";
import {
  attentionText,
  downloadText,
  ficheRefusalText,
  folderRoleText,
  portRoleText,
  proofLabel,
  registryText,
  stopTexts,
} from "@/fiche/messages";
import { forVariant, publishedPorts, readableHostPorts } from "@/fiche/ports";
import { proofOf } from "@/fiche/proof";
import { t } from "@/i18n";
import type { DeployCheck, FicheDetail, FicheQuestion } from "@/link";
import { type EngineCapacity, formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";

// « Vérifiez avant de lancer » (HRT-102, BR-FICHE-025, 026, 036) : UNE fenêtre large pour tout ce qui va partir ET la
// confirmation du socle (`AdminActDialog`, acte `service_deploy`, JAMAIS couvert par l'élévation de 5 minutes). Elle
// montre, dans l'ordre : la fiche, la variante, le nom du service, l'image exacte (version, empreinte, registre), ce que le
// programme télécharge, les ports et qui peut les joindre, les dossiers, les réponses (les secrets masqués), les
// ressources choisies avec la preuve de la recommandation, le redémarrage, l'arrêt du jeu, ce que la fiche impose, la
// phrase de confiance (jamais « isolé » ni « sûr »), puis les POINTS D'ATTENTION qui s'appliquent : chacun une case, et
// « Installer le serveur de jeu » reste inactif tant qu'un point n'est pas validé. Le contrat de licence n'est pas une
// case : le bouton ouvre d'abord sa fenêtre à deux boutons, rien ne part sans « J'accepte ». Tout est lu dans la fiche
// et la saisie ; le serveur refait chaque contrôle au lancement et a le dernier mot (BR-DEPLOY-032).
const props = defineProps<{
  open: boolean;
  serverId: string;
  detail: FicheDetail;
  draft: FicheDraft;
  questions: readonly FicheQuestion[];
  capacity: EngineCapacity | null;
  /** Les constats frais du serveur à l'ouverture ; `null` : la consultation n'a pas pu être faite. */
  check: DeployCheck | null;
  /** Le serveur vient de refuser faute de place (`freeBytes` : `null` s'il ne dit pas combien). */
  forceLowDisk: { freeBytes: number | null } | null;
  agentVersion: string | null;
  perform: (
    adminPassword: string | null,
    accept: { reservedPorts: boolean; lowDisk: boolean },
  ) => Promise<ActReport>;
}>();

const emit = defineEmits<{
  close: [];
  /** Les contrats que l'utilisateur vient d'accepter, juste avant l'envoi. */
  agree: [ids: string[]];
  imposed: [];
}>();

const ticked = ref<Record<string, boolean>>({});
watch(
  () => props.open,
  () => {
    ticked.value = {};
  },
);

const points = computed<AttentionPoint[]>(() =>
  attentionPoints({
    detail: props.detail,
    capacity: props.capacity,
    check: props.check,
    resources: props.draft.resources,
    forceLowDisk: props.forceLowDisk,
  }),
);
const attentionDone = computed(() => allValidated(points.value, ticked.value));

const published = computed(() =>
  publishedPorts(props.detail.ports, props.draft.optionalPorts, props.draft.variant),
);
const folders = computed(() => forVariant(props.detail.folders, props.draft.variant));
/** Les ports qui partent : le port du serveur choisi et celui du service, tels que la demande les dit. */
const portLines = computed(() =>
  readableHostPorts(published.value, props.draft.hostPorts).flatMap(({ id, deploy }) => {
    const port = published.value.find((item) => item.id === id);
    return port ? [{ port, deploy }] : [];
  }),
);
const variant = computed(() =>
  props.detail.variants.find((item) => item.id === props.draft.variant),
);
const delivered = computed(() =>
  props.agentVersion === null
    ? t("catalogue.deliveredWithBare")
    : t("catalogue.deliveredWith", { version: safeText(props.agentVersion) }),
);
const image = computed(() => props.detail.image);
const stop = computed(() => (props.detail.stop ? stopTexts(props.detail.stop) : []));
const downloads = computed(() => props.detail.firstStart?.downloads ?? []);

/** Une réponse telle que la fenêtre la dit : le secret masqué, un choix par son libellé, jamais une valeur secrète. */
function answerOf(question: FicheQuestion, proposed: boolean): { text: string; proposed: boolean } {
  const value = currentAnswer(question, props.draft.answers);
  if (question.secret) {
    return { text: value === "" ? t("fiche.sumEmptyAnswer") : t("fiche.secretMask"), proposed };
  }
  if (value === "") return { text: t("fiche.sumEmptyAnswer"), proposed };
  if (question.kind === "bool") {
    return {
      text: value === question.choices[0]?.value ? t("fiche.yes") : t("fiche.no"),
      proposed,
    };
  }
  const choice = question.choices.find((item) => item.value === value);
  return { text: safeText(choice ? choice.label : value), proposed };
}
const changed = computed(() => changedAnswers(props.questions, props.draft.answers));
const answers = computed(() =>
  props.questions.map((question) => ({
    question,
    ...answerOf(question, !changed.value.includes(question)),
  })),
);

const memory = computed(() =>
  props.draft.resources?.memoryBytes != null ? formatMemory(props.draft.resources.memoryBytes) : "",
);
const cpu = computed(() =>
  props.draft.resources?.cpuMillis != null ? formatCpu(props.draft.resources.cpuMillis) : "",
);
const memoryProof = computed(() => {
  const { recommendedBytes, evidence } = props.detail.resources.memory;
  const proof = proofOf(evidence);
  return recommendedBytes !== null && proof !== "unmeasured"
    ? t("fiche.sumMemoryProof", { proof: proofLabel(proof) })
    : null;
});

const reservedPorts = computed(() =>
  (props.check?.ports ?? []).filter((port) => port.status === "reserved_by_stopped"),
);
const lowDiskText = computed(() => {
  const point = points.value.find((item) => item.key === "low_disk");
  return point?.key === "low_disk" && point.freeBytes !== null
    ? t("deploy.lowDisk", { space: formatSize(point.freeBytes) })
    : t("deploy.lowDiskUnknown");
});

// ── Le contrat de licence : la fenêtre s'ouvre à « Installer le serveur de jeu », avant le mot de passe ──
const asking = ref<FicheDetail["agreements"][number] | null>(null);
const openFailed = ref(false);
let settle: ((accepted: boolean) => void) | null = null;
const links = useFicheLinks(
  () => props.serverId,
  () => props.detail.card.id,
);

/** « Lire le contrat » : le navigateur de l'utilisateur. S'il ne s'ouvre pas, la fenêtre du contrat le dit (une notification passerait sous la fenêtre modale). */
async function readAgreement(id: string): Promise<void> {
  openFailed.value = !(await links.open("agreement", id, { notify: false }));
}

function ask(agreement: FicheDetail["agreements"][number]): Promise<boolean> {
  asking.value = agreement;
  openFailed.value = false;
  return new Promise((resolve) => {
    settle = resolve;
  });
}

function answer(accepted: boolean): void {
  asking.value = null;
  settle?.(accepted);
  settle = null;
}

/** Chaque contrat à son tour ; un seul « Je refuse » annule l'envoi, rien n'est accepté à la place de l'utilisateur. */
async function beforeSubmit(): Promise<boolean> {
  const accepted: string[] = [];
  for (const agreement of props.detail.agreements) {
    if (!(await ask(agreement))) return false;
    accepted.push(agreement.id);
  }
  emit("agree", accepted);
  return true;
}

function send(adminPassword: string | null): Promise<ActReport> {
  return props.perform(adminPassword, acceptOf(points.value, ticked.value));
}

const steps = computed(() => [
  t("fiche.stepAnswers"),
  t("fiche.stepResources"),
  t("fiche.stepVerify"),
]);

function refusal(refused: { kind: string }): string | undefined {
  return ficheRefusalText(refused, props.detail) ?? refusalText(refused);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="service_deploy"
    :title="t('fiche.summaryTitle')"
    :submit-label="t('fiche.summarySubmit')"
    :cancel-label="t('fiche.summaryBack')"
    :can-submit="attentionDone"
    :before-submit="beforeSubmit"
    :perform="send"
    :refusal-text="refusal"
    wide
    @close="emit('close')"
  >
    <StepTrail :steps="steps" :current="2" :label="t('fiche.stepsLabel')" />

    <dl class="sum" data-fiche-summary>
      <div class="sum__row">
        <dt>{{ t("fiche.sumFiche") }}</dt>
        <dd data-sum-fiche>
          <span class="sum__strong">{{ safeText(detail.card.name) }}</span>
          <HTag tone="neutral">{{ delivered }}</HTag>
        </dd>
      </div>
      <div v-if="variant" class="sum__row">
        <dt>{{ t("fiche.sumVariant") }}</dt>
        <dd data-sum-variant>{{ safeText(variant.label) }}</dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumName") }}</dt>
        <dd class="sum__mono" data-sum-name>{{ draft.name }}</dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumImage") }}</dt>
        <dd data-sum-image>
          <span class="sum__mono">{{ safeText(image.repository) }}:{{ safeText(image.tag) }}</span>
          <span v-if="image.digest" class="sum__mono sum__block">{{ safeText(image.digest) }}</span>
          <span class="sum__note">{{ registryText(registryOf(image.repository)) }}</span>
        </dd>
      </div>
      <div v-if="downloads.length > 0" class="sum__row">
        <dt>{{ t("fiche.sumDownloads") }}</dt>
        <dd data-sum-downloads>
          <span v-for="(download, index) in downloads" :key="index" class="sum__block">
            {{ downloadText(download) }}
          </span>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumPorts") }}</dt>
        <dd data-sum-ports>
          <ul class="sum__list">
            <li v-for="line in portLines" :key="line.port.id" class="sum__mono">
              {{
                t("deploy.summaryPortRow", {
                  host: line.deploy.hostPort,
                  container: line.deploy.containerPort,
                  protocol: line.deploy.protocol.toUpperCase(),
                })
              }}
              <span class="sum__plain">({{ portRoleText(line.port.role) }})</span>
            </li>
          </ul>
          <span class="sum__note" data-sum-exposed>{{ t("deploy.portExposed") }}</span>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumFolders") }}</dt>
        <dd data-sum-folders>
          <ul class="sum__list">
            <li v-for="folder in folders" :key="folder.id">
              {{ folderRoleText(folder.role) }}
              <span class="sum__mono sum__block">{{ safeText(folder.target) }}</span>
            </li>
          </ul>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumAnswers") }}</dt>
        <dd data-sum-answers>
          <span v-if="answers.length === 0">{{ t("fiche.sumNoAnswers") }}</span>
          <ul v-else class="sum__list">
            <li v-for="item in answers" :key="item.question.id" data-sum-answer>
              <span class="sum__plain">{{ safeText(item.question.label) }} :</span>
              {{ item.text }}
              <HTag v-if="item.proposed && !item.question.secret" tone="neutral">
                {{ t("fiche.sumProposedTag") }}
              </HTag>
            </li>
          </ul>
          <span v-if="answers.length > 0" class="sum__note" data-sum-changed>
            {{ t("fiche.sumChanged", { n: changed.length }) }}
          </span>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumResources") }}</dt>
        <dd data-sum-resources>
          {{ t("fiche.sumResourcesText", { memory, cpu }) }}
          <span v-if="memoryProof" class="sum__note">{{ memoryProof }}</span>
        </dd>
      </div>
      <div class="sum__row">
        <dt>{{ t("fiche.sumRestart") }}</dt>
        <dd data-sum-restart>{{ t("deploy.restartText") }}</dd>
      </div>
      <div v-if="stop.length > 0" class="sum__row">
        <dt>{{ t("fiche.sumStop") }}</dt>
        <dd data-sum-stop>
          <span v-for="line in stop" :key="line" class="sum__block">{{ line }}</span>
        </dd>
      </div>
      <div v-if="detail.imposed.length > 0" class="sum__row">
        <dt>{{ t("fiche.sumImposed") }}</dt>
        <dd data-sum-imposed>
          {{ t("fiche.imposedCount", { n: detail.imposed.length }) }}
          <HButton variant="secondary" size="sm" data-imposed-open @click="emit('imposed')">
            {{ t("fiche.imposedSee") }}
          </HButton>
        </dd>
      </div>
    </dl>

    <Callout tone="info" data-sum-trust>{{ t("fiche.trust") }}</Callout>

    <section
      v-if="points.length > 0"
      class="attention"
      :aria-label="t('fiche.attention.title')"
      data-sum-attention
    >
      <div v-for="point in points" :key="point.key" class="attention__item" :data-attention="point.key">
        <Callout tone="warn" inline>
          <template v-if="point.key === 'reserved_ports'">
            <span v-for="port in reservedPorts" :key="`${port.protocol}/${port.hostPort}`" class="attention__line">
              {{ portVerdict(port).warning }}
            </span>
          </template>
          <template v-else-if="point.key === 'low_disk'">{{ lowDiskText }}</template>
          <template v-else>{{ attentionText(point) }}</template>
        </Callout>
        <HCheckbox
          :model-value="ticked[point.key] === true"
          :label="
            point.key === 'reserved_ports'
              ? t('deploy.attentionReserved')
              : point.key === 'low_disk'
                ? t('deploy.attentionDisk')
                : t('fiche.attention.understand')
          "
          @update:model-value="ticked = { ...ticked, [point.key]: $event }"
        />
      </div>
    </section>
    <p v-if="points.length > 0 && !attentionDone" class="sum__need" data-sum-need>
      {{ t("fiche.needAttention") }}
    </p>

    <AgreementDialog
      v-if="asking"
      :open="asking !== null"
      :agreement="asking"
      :open-failed="openFailed"
      @read="readAgreement(asking.id)"
      @accept="answer(true)"
      @refuse="answer(false)"
    />
  </AdminActDialog>
</template>

<style scoped>
.sum {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0;
}

.sum__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 3fr);
  gap: var(--space-3);
}

.sum__row dt {
  color: var(--tx-secondary);
}

.sum__row dd {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-2);
  min-width: 0;
  margin: 0;
  overflow-wrap: anywhere;
}

.sum__strong {
  font-weight: var(--fw-semibold);
}

.sum__mono {
  font-family: var(--font-mono);
}

.sum__plain {
  font-family: var(--font-body);
  color: var(--tx-secondary);
}

.sum__block {
  flex-basis: 100%;
}

.sum__list {
  display: flex;
  flex-direction: column;
  flex-basis: 100%;
  gap: var(--space-1);
  padding: 0;
  margin: 0;
  list-style: none;
}

.sum__note {
  flex-basis: 100%;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.attention {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.attention__item {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.attention__line {
  display: block;
}

.sum__need {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
