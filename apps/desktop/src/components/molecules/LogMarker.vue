<script setup lang="ts">
import HIcon from "../atoms/HIcon.vue";

// Une ligne spéciale d'un journal (HRT-67, BR-SLOG-006, 009, 014) : le service a redémarré, ou des lignes manquent.
// Pleine largeur, jamais sélectionnable comme du texte de journal (on ne copie pas un repère d'écran). Le texte est
// celui de l'interface, jamais une ligne du service.
defineProps<{ kind: "restart" | "gap"; text: string }>();
</script>

<template>
  <div :class="['mark', `mark--${kind}`]" data-log-marker :data-marker-kind="kind">
    <template v-if="kind === 'restart'">
      <span class="mark__rule" aria-hidden="true" />
      <HIcon name="loop" size="sm" />
      <span class="mark__text">{{ text }}</span>
      <span class="mark__rule" aria-hidden="true" />
    </template>
    <template v-else>
      <HIcon name="alert" size="sm" />
      <span class="mark__text">{{ text }}</span>
    </template>
  </div>
</template>

<style scoped>
.mark {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  box-sizing: border-box;
  height: var(--log-marker-height);
  padding: 0 var(--space-3);
  background: var(--card);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  user-select: none;
}

.mark__text {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mark__rule {
  flex: 1 1 0;
  min-width: var(--space-3);
  height: var(--border-width);
  background: var(--bd);
}

.mark--gap {
  border: var(--border-width) dashed var(--warn);
  color: var(--tx);
}

.mark--gap :deep(svg) {
  color: var(--warn);
}
</style>
