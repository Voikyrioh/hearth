<script setup lang="ts">
import { computed, nextTick, onMounted, ref, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HTextarea from "@/components/atoms/HTextarea.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import EnvVarRows from "@/components/organisms/EnvVarRows.vue";
import PortMappingRows from "@/components/organisms/PortMappingRows.vue";
import ResourceSliders from "@/components/organisms/ResourceSliders.vue";
import type { DeployFormState } from "@/composables/useDeployForm";
import { commandText, imageText, nameText, tagText } from "@/deploy/messages";
import { splitImage } from "@/deploy/rules";
import { t } from "@/i18n";
import type { DeployField } from "@/link";
import type { Resources } from "@/resources";

// Le formulaire « Déployer un service » (HRT-69, BR-DEPLOY-002 à 009, 014, 031, 033), en colonne unique : Général,
// Variables d'environnement, Ports, Commande, Ressources, puis le pied collant (raison du blocage à gauche, « Déployer »
// à droite). Les dossiers de données arrivent avec HRT-70. Il ne juge rien du serveur : les défauts de forme sont ceux
// de `deploy/rules.ts`, les constats de conflit viennent de l'agent par le composable (`form`), et tout est dit
// INDICATIF puisque le serveur refait chaque contrôle au lancement (BR-DEPLOY-032). La saisie ne va ni dans l'adresse,
// ni sur le disque : elle vit dans le magasin en mémoire seule (BR-DEPLOY-029).
const props = defineProps<{
  form: DeployFormState;
  /** Le refus du serveur au dernier envoi (BR-DEPLOY-032) : montré en tête, la saisie reste. */
  refusal: { text: string; field: DeployField | null } | null;
  /**
   * Pourquoi « Déployer » est inactif (texte écrit à gauche du bouton) ; `null` : il est actif. `fixable` : la raison
   * est « des champs à corriger », le pied propose d'aller au premier.
   */
  reason: { text: string; fixable: boolean } | null;
  busy: boolean;
}>();

const emit = defineEmits<{ submit: [] }>();

const root = ref<HTMLElement | null>(null);
const reasonId = useId();
const draft = computed(() => props.form.draft.value);

const nameError = computed(() => {
  const issue = props.form.issues.value.name;
  if (issue) return props.form.shown("name") ? nameText(issue) : undefined;
  // Le constat du serveur est immédiat (BR-DEPLOY-004) : il ne dit jamais qui tient le nom (BR-ADOPT-022).
  const status = props.form.check.value?.name;
  if (status === "taken") return t("deploy.nameTaken");
  if (status === "being_deleted") return t("deploy.nameBeingDeleted");
  return undefined;
});
const imageError = computed(() => {
  const issue = props.form.issues.value.image;
  return issue && props.form.shown("image") ? imageText(issue) : undefined;
});
const tagError = computed(() => {
  const issue = props.form.issues.value.tag;
  return issue && (props.form.shown("tag") || props.form.shown("image"))
    ? tagText(issue)
    : undefined;
});
const commandError = computed(() => {
  const issue = props.form.issues.value.command;
  return issue && props.form.shown("command") ? commandText(issue) : undefined;
});
/** Aucune version saisie : « latest » peut changer d'un jour à l'autre (BR-DEPLOY-005). */
const showsLatest = computed(
  () =>
    draft.value.tag.trim() === "" &&
    splitImage(draft.value.image).embedded === null &&
    draft.value.image.trim() !== "",
);

const resources = computed<Resources>({
  get: () => draft.value.resources ?? { memoryBytes: null, cpuMillis: null },
  set: (value) => {
    draft.value.resources = value;
  },
});

function onImageBlur() {
  props.form.moveVersion();
  props.form.touch("image");
}

/** Le curseur va au premier champ à corriger, après avoir montré tous les défauts. */
async function goToFirstIssue() {
  props.form.touchAll();
  await nextTick();
  const target = root.value?.querySelector<HTMLElement>('[aria-invalid="true"]');
  target?.focus();
  target?.scrollIntoView?.({ block: "center" });
}

/** Met le curseur sur le champ qu'un refus du serveur désigne. */
function focusField(field: DeployField) {
  const scopes: Partial<Record<DeployField, string>> = {
    name: '[data-field="name"] input',
    image: '[data-field="image"] input',
    tag: '[data-field="tag"] input',
    env: "[data-env-row] input",
    ports: "[data-port-row] input",
    command: '[data-field="command"] textarea',
    resources: "[data-resources] input",
  };
  const selector = scopes[field];
  const target = selector ? root.value?.querySelector<HTMLElement>(selector) : null;
  target?.focus();
  target?.scrollIntoView?.({ block: "center" });
}

onMounted(async () => {
  await nextTick();
  root.value?.querySelector<HTMLInputElement>('[data-field="name"] input')?.focus();
});

defineExpose({ focusField });
</script>

<template>
  <div ref="root" class="deploy" :aria-label="t('deploy.formLabel')" role="group" data-deploy-form>
    <Callout v-if="refusal" tone="error" role="alert" data-deploy-refusal>
      {{ refusal.text }}
    </Callout>

    <DashCard :title="t('deploy.general')">
      <HInput
        data-field="name"
        :model-value="draft.name"
        :label="t('deploy.nameLabel')"
        :placeholder="t('deploy.namePlaceholder')"
        :help="t('deploy.nameHelp')"
        :error="nameError"
        mono
        @update:model-value="draft.name = $event"
        @blur="form.touch('name')"
      />
      <div class="deploy__image">
        <HInput
          class="deploy__image-name"
          data-field="image"
          :model-value="draft.image"
          :label="t('deploy.imageLabel')"
          :placeholder="t('deploy.imagePlaceholder')"
          :help="t('deploy.imageHelp')"
          :error="imageError"
          mono
          @update:model-value="draft.image = $event"
          @blur="onImageBlur"
        />
        <HInput
          class="deploy__image-tag"
          data-field="tag"
          :model-value="draft.tag"
          :label="t('deploy.tagLabel')"
          :placeholder="t('deploy.tagPlaceholder')"
          :error="tagError"
          mono
          @update:model-value="draft.tag = $event"
          @blur="form.touch('tag')"
        />
      </div>
      <Callout v-if="form.versionMoved.value" tone="info" inline data-version-moved>
        {{ t("deploy.imageMoved") }}
      </Callout>
      <Callout v-if="showsLatest" tone="info" inline data-latest>{{ t("deploy.tagLatest") }}</Callout>
    </DashCard>

    <EnvVarRows
      :rows="draft.env"
      :issues="form.issues.value.env"
      :shown="form.shown"
      @add="form.addEnv()"
      @remove="form.removeEnv($event)"
      @update="form.updateEnv"
      @touch="form.touch($event)"
    />

    <PortMappingRows
      :rows="draft.ports"
      :issues="form.issues.value.ports"
      :verdicts="form.portVerdicts.value"
      :waiting="form.portsWaiting.value"
      :shown="form.shown"
      @add="form.addPort()"
      @remove="form.removePort($event)"
      @update="form.updatePort"
      @protocol="form.setProtocol"
      @suggest="form.useSuggested"
      @touch="form.touch($event)"
    />

    <DashCard :title="t('deploy.commandTitle')">
      <HTextarea
        data-field="command"
        :model-value="draft.command"
        :label="t('deploy.commandLabel')"
        :placeholder="t('deploy.commandPlaceholder')"
        :error="commandError"
        mono
        @update:model-value="draft.command = $event"
        @blur="form.touch('command')"
      />
      <Callout tone="info" data-command-secret>{{ t("deploy.commandSecret") }}</Callout>
    </DashCard>

    <DashCard :title="t('deploy.resourcesTitle')" data-resources>
      <ResourceSliders :capacity="form.capacity.value" v-model="resources" />
      <HButton
        v-if="form.capacity.value === null && form.capacityFailed.value"
        variant="secondary"
        size="sm"
        data-capacity-retry
        @click="form.loadCapacity()"
      >
        {{ t("common.retry") }}
      </HButton>
    </DashCard>

    <div class="deploy__bar" data-deploy-bar>
      <div class="deploy__reason">
        <p v-if="reason" :id="reasonId" class="deploy__reason-text" data-deploy-reason>
          {{ reason.text }}
        </p>
        <HButton
          v-if="reason?.fixable"
          variant="ghost"
          size="sm"
          data-deploy-first-issue
          @click="goToFirstIssue"
        >
          {{ t("deploy.goToField") }}
        </HButton>
      </div>
      <HButton
        size="lg"
        data-deploy-submit
        :disabled="reason !== null"
        :busy="busy"
        :described-by="reason ? reasonId : undefined"
        @click="emit('submit')"
      >
        {{ t("deploy.submit") }}
      </HButton>
    </div>
  </div>
</template>

<style scoped>
.deploy {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  width: 100%;
  max-width: var(--column-max);
  min-width: 0;
}

.deploy__image {
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(0, 1fr);
  gap: var(--space-3);
}

.deploy__bar {
  position: sticky;
  bottom: 0;
  z-index: var(--z-raised);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.deploy__reason {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.deploy__reason-text {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
