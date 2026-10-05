<script setup lang="ts">
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// Fil des étapes de l'assistant : l'étape courante en pastille braise, les étapes faites cochées.
// L'état est aussi dit en texte pour les lecteurs d'écran (« Adresse, terminée »).
defineProps<{ steps: string[]; current: number; label: string }>();
</script>

<template>
  <ol class="trail" :aria-label="label">
    <li
      v-for="(step, index) in steps"
      :key="step"
      :class="['trail__step', { 'trail__step--done': index < current, 'trail__step--now': index === current }]"
      :aria-current="index === current ? 'step' : undefined"
    >
      <span class="trail__puck" aria-hidden="true">
        <HIcon v-if="index < current" name="check" size="sm" />
        <template v-else>{{ index + 1 }}</template>
      </span>
      <span class="trail__name">{{ step }}</span>
      <span class="trail__sr">
        {{ index < current ? t("connect.stepDone") : index === current ? t("connect.stepCurrent") : "" }}
      </span>
    </li>
  </ol>
</template>

<style scoped>
.trail {
  display: flex;
  gap: var(--space-4);
  padding: 0;
  margin: 0;
  list-style: none;
}

.trail__step {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--tx3);
  font-size: var(--fs-small);
}

.trail__puck {
  display: grid;
  place-items: center;
  width: var(--icon-size);
  height: var(--icon-size);
  border: var(--border-width) solid var(--bd);
  border-radius: 50%;
  font-weight: var(--fw-semibold);
}

.trail__step--now {
  color: var(--tx);
}

.trail__step--now .trail__puck {
  border-color: var(--ac);
  background: var(--ac);
  color: var(--on-ac);
}

.trail__step--done {
  color: var(--tx2);
}

.trail__step--done .trail__puck {
  border-color: var(--ok);
  color: var(--ok);
}

.trail__sr {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
