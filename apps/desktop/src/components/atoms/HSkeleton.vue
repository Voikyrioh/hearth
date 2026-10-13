<script setup lang="ts">
// Barre esquissée d'un contenu en cours de lecture (lignes de liste) : une forme neutre, jamais de fausse
// donnée. Mouvement réduit ou non, elle reste FIXE : aucune animation (le chargement se dit par le texte
// et `aria-busy` du conteneur, pas par un scintillement).
withDefaults(defineProps<{ size?: "sm" | "md" | "lg" }>(), { size: "md" });
</script>

<template>
  <span :class="['skeleton', `skeleton--${size}`]" aria-hidden="true" />
</template>

<style scoped>
.skeleton {
  display: block;
  height: var(--skeleton-height);
  border-radius: var(--radius-control);
  background: var(--card-2);
}

.skeleton--sm {
  width: var(--skeleton-sm);
}

.skeleton--md {
  width: var(--skeleton-md);
}

.skeleton--lg {
  width: 100%;
}
</style>
