<script setup lang="ts">
import { computed, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";
import { blockText, type FieldBlock } from "@/services/console";
import type { LineClass } from "@/services/consoleLine";

// Le champ « Commande » de la console (HRT-269, BR-CONSOLE-003, 005, 006, 008, maquettes Console et ConsoleEtats) : une
// ligne de 1 à 256 caractères, « Envoyer », et PARTOUT une phrase qui dit pourquoi le champ est inactif (serveur arrêté,
// qui démarre, opération en cours, lecture seule, trop de commandes) ou pourquoi « Envoyer » l'est (ligne hors forme). La
// commande part telle quelle : Hearth ne la complète, ne la traduit ni ne la corrige. Une commande sensible le dit avant
// l'envoi : le mot de passe sera demandé.
const props = defineProps<{
  modelValue: string;
  block: FieldBlock | null;
  /** Pourquoi « Envoyer » est inactif quand le champ est actif (forme de la ligne). */
  issue: string | null;
  judged: LineClass | null;
  canSend: boolean;
  sending: boolean;
  maxChars: number;
  /** « Démarrer » : seulement pour qui peut piloter le service. */
  canStart: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  send: [];
  start: [];
  follow: [];
}>();

const inputId = useId();
const helpId = useId();
const blocked = computed(() => props.block !== null);
const text = computed(() => (props.block ? blockText(props.block) : null));
const sensitive = computed(() => (props.judged?.kind === "sensitive" ? props.judged.verb : null));
const refused = computed(() => props.judged?.kind === "refused");

function onInput(event: Event) {
  emit("update:modelValue", (event.target as HTMLTextAreaElement).value);
}
</script>

<template>
  <form class="cf" data-console-field @submit.prevent="emit('send')">
    <label :for="inputId" class="cf__label">{{ t("console.field.label") }}</label>
    <div class="cf__row">
      <span class="cf__prompt" aria-hidden="true">&gt;</span>
      <textarea
        :id="inputId"
        class="cf__input"
        rows="1"
        wrap="off"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        :value="blocked ? '' : modelValue"
        :placeholder="text?.title"
        :disabled="blocked"
        :aria-describedby="helpId"
        :aria-invalid="issue !== null ? 'true' : undefined"
        data-console-input
        @input="onInput"
        @keydown.enter.exact.prevent="emit('send')"
      ></textarea>
      <HButton
        type="submit"
        variant="primary"
        needs-link
        :disabled="!canSend"
        :busy="sending"
        :described-by="helpId"
        data-console-send
      >
        {{ t("console.field.send") }}
      </HButton>
    </div>
    <p :id="helpId" class="cf__help" data-console-help>
      <template v-if="text">
        {{ text.help }}
        <HButton v-if="block?.kind === 'stopped' && canStart" variant="ghost" size="sm" data-console-start @click="emit('start')">
          {{ t("console.field.start") }}
        </HButton>
        <HButton v-if="block?.kind === 'busy'" variant="ghost" size="sm" data-console-follow @click="emit('follow')">
          {{ t("console.field.follow") }}
        </HButton>
      </template>
      <template v-else-if="issue">{{ issue }}</template>
      <template v-else-if="refused">{{ t("console.field.refused") }}</template>
      <template v-else-if="sensitive">{{ t("console.field.sensitive", { verb: sensitive }) }}</template>
      <template v-else>{{ t("console.field.hint", { n: maxChars }) }}</template>
    </p>
  </form>
</template>

<style scoped>
.cf {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.cf__label {
  font-weight: var(--fw-semibold);
}

.cf__row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.cf__prompt {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
}

.cf__input {
  flex: 1 1 0;
  min-width: 0;
  min-height: var(--target-size);
  padding: var(--field-pad-y) var(--field-pad-x);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
  font-family: var(--font-mono);
  resize: none;
}

.cf__input:disabled {
  background: var(--disabled-bg);
  color: var(--disabled-tx);
}

.cf__input:focus-visible {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.cf__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
