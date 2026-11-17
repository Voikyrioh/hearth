<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import HInput from "@/components/atoms/HInput.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import FichePortField from "@/components/molecules/FichePortField.vue";
import QuestionField from "@/components/molecules/QuestionField.vue";
import VariantChoice from "@/components/molecules/VariantChoice.vue";
import InstallPack from "@/components/organisms/InstallPack.vue";
import InstallSettings from "@/components/organisms/InstallSettings.vue";
import WorldCard from "@/components/organisms/WorldCard.vue";
import type { FicheInstallState } from "@/composables/useFicheInstall";
import { nameTakenText, nameText, portVerdict } from "@/deploy/messages";
import { currentAnswer } from "@/fiche/answers";
import { shownHostPort } from "@/fiche/draft";
import { answerIssueText } from "@/fiche/messages";
import { optionalPorts, startingHostPort } from "@/fiche/ports";
import { advancedLabels, splitQuestions } from "@/fiche/questions";
import { t } from "@/i18n";
import type { FicheLinkKind, FichePort } from "@/link";

// Les « Réponses » de l'installation d'un serveur de jeu (HRT-102, BR-FICHE-014 à 019) : la variante (quand la fiche en a
// plusieurs), le nom du service (proposé par la fiche), les questions de la fiche (huit au plus, chacune dans le champ de
// son genre), le ou les ports du jeu, et la version de l'image, qui est celle de la fiche. Il n'y a AUCUN champ libre
// pour une variable, une commande, une image ou un dossier : ce que la fiche ne demande pas n'est pas modifiable ici.
// Un défaut de forme ne se montre qu'une fois le champ quitté (ou la page validée) ; les constats de conflit (nom
// pris, port tenu) viennent de l'agent et sont immédiats. Le serveur refait chaque contrôle au lancement.
const props = defineProps<{ install: FicheInstallState }>();

const emit = defineEmits<{ link: [kind: FicheLinkKind, key: string] }>();

const detail = computed(() => props.install.detail.value);
const draft = computed(() => props.install.draft.value);

const nameError = computed(() => {
  const issue = props.install.nameProblem.value;
  if (issue) return props.install.shown("name") ? nameText(issue) : undefined;
  // Le constat du serveur est immédiat (BR-DEPLOY-004) ; il nomme le conteneur hors Hearth qui tient le nom (BR-ADOPT-022).
  const status = props.install.check.value?.name;
  if (status === "taken") return nameTakenText(props.install.check.value?.nameHolder ?? null);
  if (status === "being_deleted") return t("deploy.nameBeingDeleted");
  return undefined;
});

function questionError(id: string): string | undefined {
  const issue = props.install.answerProblems.value.get(id);
  return issue && props.install.shown(`q:${id}`) ? answerIssueText(issue) : undefined;
}

function portError(port: FichePort): string | undefined {
  const issue = props.install.portProblems.value.get(port.id);
  if (!issue || !props.install.shown(`p:${port.id}`)) return undefined;
  if (issue === "required") return t("fiche.portRequired");
  if (issue === "not_a_number") return t("fiche.portNotNumber");
  if (issue === "duplicate") return t("fiche.portDuplicate");
  return t("fiche.portRange");
}

// L'essentiel à l'écran, le reste dans « Réglages avancés », replié avec ses valeurs proposées (HRT-244, BR-FICHE-065). Le bloc
// s'ouvre seul quand l'une de ses questions est refusée (par le client ou par le serveur) ou désignée par « Aller au champ ».
const split = computed(() => splitQuestions(props.install.questions.value, detail.value));
const advancedOpen = ref(false);
const advancedSummary = computed(() => {
  const { shown, more } = advancedLabels(split.value.advanced);
  const list = shown.map((label) => safeText(label)).join(", ");
  return more > 0 ? t("fiche.advancedMore", { list, k: more }) : t("fiche.advancedList", { list });
});
const advancedHasIssue = computed(() =>
  split.value.advanced.some((question) => questionError(question.id) !== undefined),
);
watch(advancedHasIssue, (issue) => {
  if (issue) advancedOpen.value = true;
});

const optional = computed(() =>
  detail.value ? optionalPorts(detail.value.ports, draft.value?.variant ?? null) : [],
);
const always = computed(() =>
  props.install.published.value.filter((port) => port.publish === "always"),
);

function changed(port: FichePort): boolean {
  return draft.value !== null && shownHostPort(draft.value, port) !== startingHostPort(port);
}

const hasOptional = (port: FichePort): boolean =>
  draft.value?.optionalPorts.includes(port.id) ?? false;

defineExpose({
  /** Met le curseur dans le nom du service, premier champ de la page. */
  focusName(): void {
    document
      .querySelector<HTMLInputElement>('[data-install-answers] [data-field="name"] input')
      ?.focus();
  },
  /** Met le curseur sur le champ d'une question (un refus de l'agent la désigne) ; déplie « Réglages avancés » s'il le faut. */
  async focusQuestion(id: string): Promise<HTMLElement | null> {
    if (split.value.advanced.some((question) => question.id === id)) {
      advancedOpen.value = true;
      await nextTick();
    }
    const root = document.querySelector<HTMLElement>(`[data-question="${CSS.escape(id)}"]`);
    const target = root?.querySelector<HTMLElement>("input, select, button") ?? null;
    target?.focus();
    target?.scrollIntoView?.({ block: "center" });
    return target;
  },
});
</script>

<template>
  <div v-if="detail && draft" class="answers" data-install-answers>
    <DashCard v-if="detail.variants.length > 1" :title="t('fiche.cardVariant')">
      <VariantChoice
        :variants="detail.variants"
        :model-value="draft.variant ?? ''"
        @update:model-value="install.setVariant($event)"
      />
    </DashCard>

    <!-- L'essentiel d'abord, dans UNE carte : le nom, les questions qui comptent, les ports (maquette Ajout2Reglages). -->
    <DashCard :title="t('fiche.cardQuestionsAsk')">
      <HInput
        data-field="name"
        :model-value="draft.name"
        :label="t('fiche.cardName')"
        :help="t('fiche.nameHelp')"
        :error="nameError"
        mono
        @update:model-value="install.setName($event)"
        @blur="install.touch('name')"
      />
      <QuestionField
        v-for="question in split.essential"
        :key="question.id"
        :question="question"
        :model-value="currentAnswer(question, draft.answers)"
        :error="questionError(question.id)"
        @update:model-value="install.setAnswer(question.id, $event)"
        @blur="install.touch(`q:${question.id}`)"
        @link="emit('link', 'token', question.id)"
      />
      <FichePortField
        v-for="port in always"
        :key="port.id"
        :port="port"
        :model-value="shownHostPort(draft, port)"
        :error="portError(port)"
        :verdict="install.portVerdicts.value.get(port.id) ?? portVerdict(undefined)"
        :changed="changed(port)"
        :waiting="install.portsWaiting.value"
        @update:model-value="install.setHostPort(port.id, $event)"
        @blur="install.touch(`p:${port.id}`)"
        @suggest="install.setHostPort(port.id, String($event))"
      />
      <FichePortField
        v-for="port in optional"
        :key="port.id"
        :port="port"
        :model-value="shownHostPort(draft, port)"
        :error="portError(port)"
        :verdict="install.portVerdicts.value.get(port.id) ?? portVerdict(undefined)"
        :changed="changed(port)"
        :waiting="install.portsWaiting.value"
        optional
        :on="hasOptional(port)"
        @update:on="install.setOptionalPort(port.id, $event)"
        @update:model-value="install.setHostPort(port.id, $event)"
        @blur="install.touch(`p:${port.id}`)"
        @suggest="install.setHostPort(port.id, String($event))"
      />
    </DashCard>

    <!-- Le monde : neuf, ou repris d'un monde existant (HRT-106) -->
    <WorldCard :install="install" />
    <!-- Le pack de mods (HRT-125) : seulement si la fiche offre le dépôt d'un pack -->
    <InstallPack :install="install" />

    <!-- Les réglages avancés, repliés avec leurs valeurs proposées ; UN seul moyen d'ouvrir tous les autres réglages. -->
    <DashCard v-if="split.advanced.length > 0 || install.extras.value.length > 0" :title="t('fiche.cardAdvanced')">
      <details
        v-if="split.advanced.length > 0"
        class="answers__advanced"
        data-install-advanced
        :open="advancedOpen"
        @toggle="advancedOpen = ($event.target as HTMLDetailsElement).open"
      >
        <summary class="answers__advanced-title" data-install-advanced-summary>
          {{ advancedSummary }}
        </summary>
        <p class="answers__advanced-help">{{ t("fiche.advancedHelp") }}</p>
        <QuestionField
          v-for="question in split.advanced"
          :key="question.id"
          :question="question"
          :model-value="currentAnswer(question, draft.answers)"
          :error="questionError(question.id)"
          @update:model-value="install.setAnswer(question.id, $event)"
          @blur="install.touch(`q:${question.id}`)"
          @link="emit('link', 'token', question.id)"
        />
      </details>
      <!-- « Voir tous les réglages » (HRT-109, BR-FICHE-040) : les autres réglages du jeu, repliés par défaut -->
      <InstallSettings :install="install" />
    </DashCard>

    <p class="answers__version" data-install-version>
      {{ t("fiche.versionFixed", { version: safeText(detail.image.tag) }) }}
    </p>
  </div>
</template>

<style scoped>
.answers {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.answers__advanced {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.answers__advanced-title {
  color: var(--tx);
  cursor: pointer;
  font-weight: var(--fw-semibold);
}

.answers__advanced-help {
  margin: 0;
  color: var(--tx-secondary);
}

.answers__version {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
