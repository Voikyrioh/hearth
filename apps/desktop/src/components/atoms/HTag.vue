<script setup lang="ts">
// Étiquette courte (rôle, état). Le sens ne repose jamais sur la couleur seule : le texte le porte.
withDefaults(defineProps<{ tone?: "neutral" | "accent" | "ok" | "warn" | "crit" }>(), {
  tone: "neutral",
});
</script>

<template>
  <span :class="['tag', `tag--${tone}`]"><slot /></span>
</template>

<style scoped>
.tag {
  display: inline-block;
  padding: 2px var(--space-2);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  color: var(--tx2);
  font-size: var(--fs-small);
  white-space: nowrap;
}

.tag--accent {
  border-color: var(--ac);
  color: var(--ac);
}

.tag--ok {
  border-color: var(--ok);
  color: var(--ok);
}

.tag--warn {
  border-color: var(--warn);
  color: var(--warn);
}

.tag--crit {
  border-color: var(--crit);
  color: var(--crit);
}
</style>
