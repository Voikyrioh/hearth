<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import { useServerAction } from "@/composables/useServerAction";
import { getLinkBridge, SimulatedLinkBridge } from "@/link";
import { useServersStore } from "@/stores/servers";

// Panneau de développement : lance une action d'essai du pont SIMULÉ (jamais du pont réel : aucune
// commande générique n'existe dans la coquille) sur le serveur affiché, comme le fera un vrai bouton
// d'administration (même composable, même règle « le serveur est requis »). Le mode de l'action
// (réponse, ou lien coupé avant la réponse) se pilote par `__hearthSim.actionMode`. Chargé
// seulement en mode développement, jamais dans le binaire livré. Libellés hors `fr.ts` : outil de
// développement.
const LABEL = "Lancer une action";
const bridge = getLinkBridge();
const sim = bridge instanceof SimulatedLinkBridge ? bridge : null;
const hidden = new URLSearchParams(window.location.search).has("nodev");
const servers = useServersStore();
const action = useServerAction();

async function launch() {
  const id = servers.current?.id;
  if (id && sim) await action.run(() => sim.runDevAction(id));
}
</script>

<template>
  <div v-if="sim && !hidden" class="devaction">
    <HButton
      size="sm"
      variant="secondary"
      needs-link
      :busy="action.busy.value"
      data-sim-action
      @click="launch"
    >
      {{ LABEL }}
    </HButton>
  </div>
</template>

<style scoped>
.devaction {
  position: fixed;
  right: calc(var(--toast-width) + var(--space-4));
  bottom: var(--space-3);
  z-index: var(--z-dev);
  padding: var(--space-2) var(--space-3);
  border: var(--border-width) dashed var(--bd);
  border-radius: var(--radius-control);
  background: var(--card);
}
</style>
