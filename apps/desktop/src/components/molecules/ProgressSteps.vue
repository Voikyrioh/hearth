<script setup lang="ts">
import HIcon from "@/components/atoms/HIcon.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import { t } from "@/i18n";

// Les étapes d'une opération, une à la fois (HRT-69, BR-DEPLOY-016) : faite = coche, en cours = point braise, à venir =
// cercle vide. L'état est aussi dit en texte pour les lecteurs d'écran (« Vérification, faite »). L'étape qui progresse
// montre une barre : à pourcentage connu, une jauge bornée de 0 à 100 ; sinon une barre indéterminée. Rien n'est calculé
// ici : l'agent donne l'étape et la progression, l'appelant donne les libellés.
export interface ProgressStep {
  key: string;
  label: string;
  state: "done" | "now" | "later";
  /** Seulement pour l'étape qui progresse : la jauge (`null` : indéterminée), le texte « 40 % · 400 Mo sur 1 Go ». */
  bar?: { ratio: number | null; text: string | null };
}

defineProps<{ steps: readonly ProgressStep[]; label: string }>();

const STATE_KEYS = {
  done: "deploy.stepDone",
  now: "deploy.stepNow",
  later: "deploy.stepLater",
} as const;
</script>

<template>
  <ol class="steps" :aria-label="label" data-progress-steps>
    <li
      v-for="step in steps"
      :key="step.key"
      :class="['steps__item', `steps__item--${step.state}`]"
      :data-step="step.key"
      :data-state="step.state"
      :aria-current="step.state === 'now' ? 'step' : undefined"
    >
      <span class="steps__mark" aria-hidden="true">
        <HIcon v-if="step.state === 'done'" name="check" size="sm" />
        <i v-else-if="step.state === 'now'" class="steps__dot" />
      </span>
      <span class="steps__label">
        {{ step.label }}
        <span class="sr-only">, {{ t(STATE_KEYS[step.state]) }}</span>
      </span>
      <div v-if="step.bar" class="steps__bar">
        <HMeter
          :ratio="step.bar.ratio"
          :indeterminate="step.bar.ratio === null"
          :label="step.label"
        />
        <span v-if="step.bar.text" class="steps__text" data-progress-text>{{ step.bar.text }}</span>
      </div>
    </li>
  </ol>
</template>

<style scoped>
.steps {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
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
  color: var(--tx-secondary);
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
  display: flex;
  flex-direction: column;
  grid-column: 2;
  gap: var(--space-1);
}

.steps__text {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
}
</style>
