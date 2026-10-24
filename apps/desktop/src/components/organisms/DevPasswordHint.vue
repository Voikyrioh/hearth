<script setup lang="ts">
import { computed } from "vue";
import { getLinkBridge, SimulatedLinkBridge } from "@/link";

// Rappel du mot de passe du compte simulé, sous le champ de confirmation d'un acte (revue UX 2026-10-24, constat 1).
// Visible seulement avec le pont SIMULÉ : `AdminActDialog` ne le charge que derrière `import.meta.env.DEV`, donc
// il n'existe pas dans le binaire livré (`npm run check:dist` refuse le mot de passe et ce libellé dans `dist/`).
// Outil de développement : son libellé reste ici (hors `fr.ts`) pour ne pas entrer dans le bundle livré.
const props = defineProps<{ serverId: string }>();
const bridge = getLinkBridge();
const password = computed(() =>
  bridge instanceof SimulatedLinkBridge ? bridge.simulatedPassword(props.serverId) : null,
);
</script>

<template>
  <p v-if="password" class="dev-password" data-dev-password>
    Simulateur : le mot de passe de ce compte est <code>{{ password }}</code>.
  </p>
</template>

<style scoped>
.dev-password {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.dev-password code {
  font-family: var(--font-mono);
}
</style>
