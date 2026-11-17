<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";

// Un chemin de l'écran « Ajouter un serveur de jeu » (HRT-243, BR-DEPLOY-039) : un titre, une étiquette facultative, un
// texte, une note et un bouton. Quand le bouton est inactif, sa raison est écrite dessous (`reason`), jamais seulement en
// infobulle. Le texte du bouton ne passe jamais à la ligne (HRT-249).
defineProps<{
  title: string;
  /** « Conseillé », « Pour les habitués » : facultative. */
  tag?: string;
  text: string;
  note: string;
  button: string;
  primary?: boolean;
  disabled?: boolean;
  /** Pourquoi le bouton est inactif ; écrit sous le bouton. */
  reason?: string;
}>();

defineEmits<{ go: [] }>();
</script>

<template>
  <article class="path" data-add-path>
    <header class="path__head">
      <h2 class="path__title">{{ title }}</h2>
      <HTag v-if="tag" :tone="primary ? 'accent' : 'neutral'" data-add-path-tag>{{ tag }}</HTag>
    </header>
    <p class="path__text">{{ text }}</p>
    <p class="path__note">{{ note }}</p>
    <HButton
      class="path__button"
      :variant="primary ? 'primary' : 'secondary'"
      :disabled="disabled"
      :hint="reason"
      :reason-below="disabled && reason !== undefined"
      data-add-path-go
      @click="$emit('go')"
    >
      {{ button }}
    </HButton>
  </article>
</template>

<style scoped>
.path {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
  min-width: 0;
}

.path__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2) var(--space-3);
}

.path__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.path__text {
  margin: 0;
}

.path__note {
  margin: 0;
  color: var(--tx-secondary);
}

.path__button {
  align-self: flex-start;
  margin-top: auto;
}
</style>
