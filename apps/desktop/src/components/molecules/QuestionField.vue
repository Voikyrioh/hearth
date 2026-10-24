<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HInput from "@/components/atoms/HInput.vue";
import HPasswordInput from "@/components/atoms/HPasswordInput.vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import Callout from "@/components/molecules/Callout.vue";
import { boundsText, charsetHint } from "@/fiche/messages";
import { t } from "@/i18n";
import type { FicheQuestion } from "@/link";

// Une question d'installation d'une fiche (HRT-102, BR-FICHE-016, BR-FICHE-017, BR-FICHE-059) : le champ qui convient à
// son GENRE, tel que la fiche le dit. Texte : champ de texte ; nombre entier : champ numérique sans molette ; choix :
// boutons exclusifs jusqu'à quatre choix, sinon liste ; oui ou non : deux boutons ; secret : champ masqué avec
// « Afficher / Masquer ». Les libellés, l'aide, les bornes, les choix et la mention « Ne pourra plus être changé après
// la création du monde » viennent de la fiche : AUCUNE règle de jeu ici (écart 14.26). Un genre que ce client ne connaît
// pas devient un champ de texte et l'agent juge. Un mot de passe de jeu laissé vide avertit sans bloquer
// (BR-JEU-027) ; un jeton de serveur dit où le créer, sans jamais ouvrir l'adresse de lui-même.
const props = defineProps<{
  question: FicheQuestion;
  modelValue: string;
  /** Le défaut à montrer sous le champ (déjà en phrase) ; `undefined` : aucun. */
  error?: string;
  disabled?: boolean;
  /**
   * Un secret DÉFINI sur le serveur dont la valeur n'est jamais lue (onglet « Réglages », HRT-109) : un champ vide veut
   * alors dire « inchangé », pas « aucun mot de passe » ; les avertissements d'un champ vide ne s'appliquent pas.
   */
  secretKept?: boolean;
  /** Ne pas dire « Ne pourra plus être changé après la création du monde » : l'appelant le dit déjà (onglet « Réglages »). */
  hideFrozen?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  blur: [];
  link: [];
}>();

const label = computed(() => safeText(props.question.label));
const help = computed(() => {
  const parts = [safeText(props.question.help)];
  const hint = charsetHint(props.question) ?? boundsText(props.question);
  if (hint) parts.push(hint);
  if (props.question.secret) parts.push(t("fiche.secretHint"));
  return parts.filter((part) => part !== "").join(" ");
});

const isToken = computed(
  () => props.question.secretKind === "server_token" && props.question.obtainUrl !== null,
);
/** Un mot de passe de jeu laissé vide : toute personne qui joint le port peut rejoindre la partie. */
const openGame = computed(
  () =>
    props.question.secretKind === "game_password" &&
    props.modelValue === "" &&
    props.secretKept !== true &&
    props.error === undefined,
);
const tokenNeeded = computed(
  () =>
    props.question.secretKind === "server_token" &&
    (props.question.minLen ?? 0) > 0 &&
    props.secretKept !== true &&
    props.modelValue === "",
);

const choices = computed(() =>
  props.question.kind === "bool"
    ? [
        { value: props.question.choices[0]?.value ?? "", label: t("fiche.yes") },
        { value: props.question.choices[1]?.value ?? "", label: t("fiche.no") },
      ]
    : props.question.choices.map((choice) => ({
        value: choice.value,
        label: safeText(choice.label),
      })),
);
/** Jusqu'à quatre choix : des boutons exclusifs ; au-delà, une liste. */
const segmented = computed(
  () => props.question.kind === "bool" || props.question.choices.length <= 4,
);
const pick = computed(() => props.question.kind === "choice" || props.question.kind === "bool");
</script>

<template>
  <div class="field" data-question-field :data-question="question.id">
    <template v-if="pick && segmented">
      <p class="field__label">{{ label }}</p>
      <HSegmented
        :model-value="modelValue"
        :options="choices"
        :label="label"
        :disabled="disabled"
        @update:model-value="emit('update:modelValue', $event)"
      />
      <p v-if="help" class="field__help">{{ help }}</p>
    </template>
    <template v-else-if="pick">
      <HSelect
        :model-value="modelValue"
        :label="label"
        :options="choices"
        :disabled="disabled"
        @update:model-value="emit('update:modelValue', $event)"
      />
      <p v-if="help" class="field__help">{{ help }}</p>
    </template>
    <HPasswordInput
      v-else-if="question.secret"
      :model-value="modelValue"
      :label="label"
      :help="help"
      :error="error"
      :disabled="disabled"
      @update:model-value="emit('update:modelValue', $event)"
    />
    <HInput
      v-else
      :model-value="modelValue"
      :label="label"
      :help="help"
      :error="error"
      :disabled="disabled"
      :inputmode="question.kind === 'int' ? 'numeric' : undefined"
      :mono="question.kind === 'int'"
      @update:model-value="emit('update:modelValue', $event)"
      @blur="emit('blur')"
    />
    <p v-if="error && pick" class="field__error" role="alert">{{ error }}</p>
    <p v-if="question.frozenAfterCreation && hideFrozen !== true" class="field__frozen" data-question-frozen>
      <HIcon name="lock" size="sm" />
      {{ t("fiche.frozen") }}
    </p>
    <div v-if="isToken" class="field__token" data-question-token>
      <p class="field__help">{{ t("fiche.tokenHelp") }}</p>
      <code class="field__url" data-question-token-url>{{ question.obtainUrl }}</code>
      <HButton variant="secondary" size="sm" data-question-token-open @click="emit('link')">
        <HIcon name="external" size="sm" />
        {{ t("fiche.tokenOpen") }}
      </HButton>
    </div>
    <Callout v-if="tokenNeeded" tone="warn" inline data-question-token-needed>
      {{ t("fiche.tokenNeeded") }}
    </Callout>
    <Callout v-if="openGame" tone="warn" inline data-question-open-game>
      {{ t("fiche.noPassword") }}
    </Callout>
  </div>
</template>

<style scoped>
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.field__label {
  margin: 0;
  font-weight: var(--fw-medium);
}

.field__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.field__error {
  margin: 0;
  color: var(--crit);
  font-size: var(--fs-small);
}

.field__frozen {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--warn);
  font-size: var(--fs-small);
}

.field__token {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.field__url {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}
</style>
