<script setup lang="ts">
import { computed } from "vue";
import { stepLabel } from "@/agentUpdate/messages";
import HIcon from "@/components/atoms/HIcon.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import { t } from "@/i18n";
import { UPDATE_STEPS, type UpdateStep } from "@/link";

// Les étapes d'une mise à jour de l'agent, une à la fois (BR-UPDATE-013) : faite = coche, en cours =
// point braise, à venir = cercle vide ; une barre de progression pendant le téléchargement. L'état est
// aussi dit en texte pour les lecteurs d'écran (« Vérification…, en cours »). Le pourcentage vient de
// l'agent : borné ici à 0 à 100 pour la barre, jamais calculé.
const props = defineProps<{ step: Exclude<UpdateStep, "done">; percent: number | null }>();

const current = computed(() => UPDATE_STEPS.indexOf(props.step));
const ratio = computed(() =>
  props.percent === null ? null : Math.min(100, Math.max(0, props.percent)) / 100,
);

function state(index: number): "done" | "now" | "later" {
  return index < current.value ? "done" : index === current.value ? "now" : "later";
}
</script>

<template>
  <ol class="steps" :aria-label="t('agentUpdate.stepsLabel')" data-agent-steps>
    <li
      v-for="(step, index) in UPDATE_STEPS"
      :key="step"
      :class="['steps__item', `steps__item--${state(index)}`]"
      :data-step="step"
      :data-state="state(index)"
      :aria-current="state(index) === 'now' ? 'step' : undefined"
    >
      <span class="steps__mark" aria-hidden="true">
        <HIcon v-if="state(index) === 'done'" name="check" size="sm" />
        <i v-else-if="state(index) === 'now'" class="steps__dot" />
      </span>
      <span class="steps__label">
        {{ stepLabel(step, step === props.step ? percent : null) }}
        <span class="steps__sr">, {{ t(`agentUpdate.${state(index) === "done" ? "stepDone" : state(index) === "now" ? "stepNow" : "stepLater"}`) }}</span>
      </span>
      <HMeter
        v-if="step === 'download' && state(index) === 'now' && ratio !== null"
        class="steps__bar"
        :ratio="ratio"
        :label="stepLabel('download', percent)"
      />
    </li>
  </ol>
</template>

<style scoped>
.steps {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: 0;
  margin: 0;
  list-style: none;
}

.steps__item {
  display: grid;
  grid-template-columns: var(--icon-size) 1fr;
  align-items: center;
  gap: var(--space-2) var(--space-3);
  color: var(--tx3);
}

.steps__mark {
  display: grid;
  place-items: center;
  width: var(--icon-size);
  height: var(--icon-size);
  border: var(--border-width) solid var(--bd);
  border-radius: 50%;
}

.steps__item--done {
  color: var(--tx2);
}

.steps__item--done .steps__mark {
  border-color: var(--ok);
  color: var(--ok);
}

.steps__item--now {
  color: var(--tx);
  font-weight: var(--fw-medium);
}

.steps__item--now .steps__mark {
  border-color: var(--ac);
}

.steps__dot {
  width: var(--space-2);
  height: var(--space-2);
  border-radius: 50%;
  background: var(--ac);
}

.steps__bar {
  grid-column: 2;
}

.steps__sr {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
