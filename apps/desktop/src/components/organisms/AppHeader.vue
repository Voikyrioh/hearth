<script setup lang="ts">
import LinkStatePill from "@/components/molecules/LinkStatePill.vue";
import { useLinkStore } from "@/stores/link";

// En-tête de contenu : titre de la vue, puis à droite l'état du lien du serveur, toujours
// visible (BR-RESIL-001), et un emplacement d'actions.
defineProps<{ title: string; serverId?: string }>();

const link = useLinkStore();
</script>

<template>
  <header class="head">
    <h1 class="head__title">{{ title }}</h1>
    <span class="head__gap" />
    <div v-if="$slots.actions" class="head__actions"><slot name="actions" /></div>
    <LinkStatePill v-if="serverId" :state="link.stateOf(serverId)" />
  </header>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}

.head__title {
  font-size: var(--fs-h1);
  font-weight: var(--fw-bold);
  line-height: 1;
}

.head__gap {
  flex: 1;
}

.head__actions {
  display: flex;
  gap: var(--space-3);
}
</style>
