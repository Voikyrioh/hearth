<script setup lang="ts">
import { computed, useId } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import HTextarea from "@/components/atoms/HTextarea.vue";
import { t } from "@/i18n";
import { controlValue, type Edit, isModified, resettable } from "@/settings/edits";
import { defaultLine, effectText, proposedLine } from "@/settings/messages";
import { defaultUnverified, isSecret, type SettingModel } from "@/settings/model";
import Callout from "./Callout.vue";
import IdListSetting from "./IdListSetting.vue";
import QuestionField from "./QuestionField.vue";

// Une ligne de réglage (HRT-109, BR-FICHE-037, 041, 042, 044, 057), la même dans l'onglet « Réglages » et dans « Voir
// tous les réglages » de l'installation : le libellé en mots simples et l'explication de la fiche, le nom technique du
// jeu en petit, le contrôle de son genre (`QuestionField`, `IdListSetting`, un champ de texte pour un texte que seul le
// jeu sait lire), le défaut du jeu et la valeur proposée, le repère « Modifié », la mention d'effet (une des quatre) et
// « Remettre la valeur proposée ». Un réglage figé, imposé, retiré ou qui se change en recréant le service est affiché
// avec sa raison et son contrôle grisé. AUCUNE règle de jeu ici : tout est lu dans le réglage de la fiche. Un secret
// n'est jamais lu : « Défini » ou non, et ce qu'on saisit part tel quel.
const props = defineProps<{
  model: SettingModel;
  edit: Edit | undefined;
  /** `service` : l'onglet d'un service installé ; `install` : l'installation, rien n'est encore écrit nulle part. */
  mode: "service" | "install";
  /** Le défaut de forme de la valeur saisie, déjà en phrase. */
  issue?: string;
  /** Rien ne se change (lien coupé). */
  disabled?: boolean;
}>();

const emit = defineEmits<{
  set: [value: string];
  unset: [];
  reset: [];
  blur: [];
  link: [];
}>();

const notesId = useId();
const question = computed(() => props.model.setting.question);
const label = computed(() => safeText(question.value.label));
const secret = computed(() => isSecret(props.model.setting));
const imposed = computed(() => props.model.lock === "imposed");
const locked = computed(() => props.model.lock !== null);
const readOnly = computed(() => locked.value || props.disabled === true);
const modified = computed(() => isModified(props.model, props.edit));
const kind = computed(() => question.value.kind);
const isList = computed(() => kind.value === "id_list");
const isOpaque = computed(() => kind.value === "opaque");
const value = computed(() => controlValue(props.model, props.edit));
/** Un secret défini sur le serveur, sans nouvelle saisie : il reste tel quel. */
const secretKept = computed(
  () => secret.value && props.model.state === "secret_set" && props.edit === undefined,
);
const secretRemoved = computed(() => secret.value && props.edit?.action === "unset");
const typedSecret = computed(() => (props.edit?.action === "set" ? props.edit.value : ""));
const shownValue = computed(() => (secret.value ? typedSecret.value : value.value));

const proposed = computed(() => proposedLine(props.model));
const gameDefault = computed(() => defaultLine(props.model));
const unverified = computed(
  () =>
    defaultUnverified(props.model) && props.model.state === "unwritten" && props.edit === undefined,
);
const notWritten = computed(
  () =>
    !secret.value &&
    props.mode === "service" &&
    props.model.state === "unwritten" &&
    props.edit === undefined &&
    !unverified.value,
);
// Un réglage qui attend la recréation dit déjà « Cela arrive bientôt » : sa mention d'effet serait une promesse de plus.
const effect = computed(() =>
  props.mode === "service" && props.model.lock !== "recreate"
    ? effectText(props.model.effect, props.model.lock === "frozen")
    : null,
);
const hasProposal = computed(() => !secret.value && question.value.proposed !== null);
const canReset = computed(() => !readOnly.value && resettable(props.model, props.edit));
const resetLabel = computed(() =>
  hasProposal.value ? t("gameSettings.resetOne") : t("gameSettings.resetNoProposal"),
);
const resetAria = computed(() =>
  t(hasProposal.value ? "gameSettings.resetOneAria" : "gameSettings.resetNoProposalAria", {
    label: label.value,
  }),
);
const lockText = computed(() => {
  switch (props.model.lock) {
    case "imposed":
      return t("gameSettings.imposedWhy", { why: safeText(props.model.setting.imposedWhy ?? "") });
    // Figé : la mention d'effet (BR-FICHE-042) le dit une fois, pas deux.
    case "frozen":
      return null;
    case "recreate":
      return t("gameSettings.recreateSoon");
    case "gone": {
      const until = props.model.setting.until;
      return until === null
        ? t("gameSettings.goneSentenceBare")
        : t("gameSettings.goneSentence", { version: safeText(until) });
    }
    default:
      return null;
  }
});

function update(next: string) {
  if (secret.value) {
    // Un champ de mot de passe vidé n'est pas un changement : retirer le secret est un geste à part.
    if (next === "") emit("reset");
    else emit("set", next);
    return;
  }
  emit("set", next);
}
</script>

<template>
  <fieldset
    :class="['row', { 'row--locked': locked, 'row--modified': modified }]"
    :disabled="disabled === true ? true : undefined"
    :aria-describedby="notesId"
    :data-setting="model.id"
    :data-modified="modified ? '' : undefined"
    :data-locked="model.lock ?? undefined"
  >
    <legend class="sr-only">{{ label }}</legend>

    <div class="row__tags">
      <code class="row__key" data-setting-key>{{ t("gameSettings.technicalName", { key: safeText(model.setting.key) }) }}</code>
      <HTag v-if="modified && !imposed" tone="accent" data-setting-tag="modified">{{ t("gameSettings.tagModified") }}</HTag>
      <HTag v-if="model.lock === 'frozen'" data-setting-tag="frozen">
        <HIcon name="lock" size="sm" />
        {{ t("gameSettings.tagFrozen") }}
      </HTag>
      <HTag v-if="imposed" data-setting-tag="imposed">
        <HIcon name="lock" size="sm" />
        {{ t("gameSettings.tagImposed") }}
      </HTag>
    </div>

    <template v-if="imposed">
      <p class="row__label" data-setting-label>{{ label }}</p>
      <p v-if="question.help" class="row__help">{{ safeText(question.help) }}</p>
    </template>
    <IdListSetting
      v-else-if="isList"
      :setting="model.setting"
      :model-value="value"
      :disabled="readOnly"
      @update:model-value="emit('set', $event)"
    />
    <HTextarea
      v-else-if="isOpaque"
      :model-value="value"
      :label="label"
      :help="safeText(question.help)"
      :error="issue"
      :disabled="readOnly"
      mono
      :rows="3"
      :max-rows="8"
      @update:model-value="emit('set', $event)"
    />
    <QuestionField
      v-else
      :question="question"
      :hide-frozen="mode === 'service'"
      :model-value="shownValue"
      :error="issue"
      :disabled="readOnly"
      :secret-kept="secretKept"
      @update:model-value="update"
      @blur="emit('blur')"
      @link="emit('link')"
    />

    <div :id="notesId" class="row__notes">
      <p v-if="secret && mode === 'service' && !imposed" class="row__note" data-setting-secret>
        {{ secretRemoved ? t("gameSettings.secretNotSet") : secretKept ? t("gameSettings.secretSet") : t("gameSettings.secretNotSet") }}
        <HButton
          v-if="secretKept && !readOnly"
          variant="ghost"
          size="sm"
          data-setting-secret-remove
          @click="emit('unset')"
        >
          {{ t("gameSettings.secretRemove") }}
        </HButton>
      </p>
      <p v-if="!imposed && gameDefault" class="row__note" data-setting-default>{{ gameDefault }}</p>
      <p v-if="!imposed && proposed" class="row__note" data-setting-proposed>{{ proposed }}</p>
      <Callout v-if="unverified && mode === 'service'" tone="info" inline data-setting-unverified>
        {{ t("gameSettings.unverifiedHint") }}
      </Callout>
      <p v-if="notWritten" class="row__note" data-setting-not-written>{{ t("gameSettings.notWritten") }}</p>
      <p v-if="lockText" class="row__lock" data-setting-lock>
        <HIcon name="lock" size="sm" />
        {{ lockText }}
      </p>
      <HTag v-if="effect" data-setting-effect>{{ effect }}</HTag>
    </div>

    <div v-if="canReset" class="row__actions">
      <HButton
        variant="ghost"
        size="sm"
        :aria-label="resetAria"
        data-setting-reset
        @click="emit('reset')"
      >
        {{ resetLabel }}
      </HButton>
    </div>
  </fieldset>
</template>

<style scoped>
.row {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  margin: 0;
  padding: var(--space-3) 0;
  border: 0;
  border-bottom: var(--border-width) solid var(--bd);
}

.row:last-child {
  border-bottom: 0;
}

.row--locked {
  color: var(--tx-secondary);
}

.row__tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.row__key {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.row__label {
  margin: 0;
  font-weight: var(--fw-medium);
}

.row__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.row__notes {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
}

.row__note {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.row__lock {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--warn);
  font-size: var(--fs-small);
}

.row__actions {
  display: flex;
  justify-content: flex-start;
}
</style>
