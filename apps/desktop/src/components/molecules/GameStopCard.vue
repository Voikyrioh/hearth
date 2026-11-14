<script setup lang="ts">
import { computed } from "vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { t } from "@/i18n";
import type { GameView } from "@/link";
import { stopCardOf } from "@/services/game";

// La carte « Arrêt du jeu » du détail d'un serveur de jeu (HRT-104, BR-JEU-010) : ce que le jeu fait de son monde en
// s'arrêtant, lue dans la fiche, et la phrase qui reste écrite en permanence : un arrêt qui ne passe pas par Hearth
// n'enregistre le monde que par l'enregistrement automatique du jeu. Visible de tous les profils.
const props = defineProps<{ view: GameView }>();
const card = computed(() => stopCardOf(props.view));
</script>

<template>
  <DashCard :title="t('game.stopCard.title')" data-game-stop>
    <p class="stop__line" data-game-stop-main>{{ card.main }}</p>
    <p v-if="card.outside" class="stop__line stop__line--soft" data-game-stop-outside>
      {{ card.outside }}
    </p>
    <p class="stop__line stop__line--soft" data-game-stop-grace>{{ card.grace }}</p>
  </DashCard>
</template>

<style scoped>
.stop__line {
  margin: 0;
  overflow-wrap: anywhere;
}

.stop__line--soft {
  color: var(--tx-secondary);
}
</style>
