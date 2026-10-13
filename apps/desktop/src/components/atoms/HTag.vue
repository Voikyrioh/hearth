<script setup lang="ts">
// Étiquette courte (rôle, état). Le sens ne repose jamais sur la couleur seule : le texte le porte.
// `interactive` : l'étiquette est un bouton (clavier, focus visible) qui ouvre une explication ; le clic est
// reçu par l'appelant (`@click` passe à la racine).
withDefaults(
  defineProps<{ tone?: "neutral" | "accent" | "ok" | "warn" | "crit"; interactive?: boolean }>(),
  { tone: "neutral", interactive: false },
);
</script>

<template>
  <button
    v-if="interactive"
    type="button"
    :class="['tag', `tag--${tone}`, 'tag--interactive']"
  >
    <slot />
  </button>
  <span v-else :class="['tag', `tag--${tone}`]"><slot /></span>
</template>

<style scoped>
.tag {
  display: inline-block;
  padding: var(--space-half) var(--space-2);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  white-space: nowrap;
}

.tag--interactive {
  background: transparent;
  cursor: pointer;
  font-family: inherit;
}

.tag--interactive:hover {
  background: var(--card-2);
  color: var(--tx);
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
