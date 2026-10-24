<script setup lang="ts">
import { computed } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import { type FirstStartFacts, firstStartView } from "@/fiche/firstStart";
import { t } from "@/i18n";
import type { FicheFirstStart } from "@/link";

// Le premier démarrage long d'un serveur de jeu (HRT-102, BR-FICHE-056) : un encart d'information, en tête de la pile
// d'alertes du service, qui ne dit QUE des faits. « Premier démarrage : le jeu prépare ses fichiers. Ce n'est pas une
// panne. », l'étape que la fiche sait reconnaître dans les journaux quand Hearth en a vu une, les octets de fichiers de
// jeu déjà écrits (relevés toutes les 30 secondes, jamais animés entre deux relevés) avec le total attendu quand la
// fiche le connaît avec une preuve. JAMAIS de barre, de pourcentage, de durée restante ni de compteur qui défile :
// Hearth ne connaît pas la progression réelle. Quand la durée annoncée par la fiche est dépassée, le panneau est
// remplacé par « Hearth ne peut pas savoir si le jeu est prêt. » : rien n'est arrêté ni relancé. Le panneau est annoncé
// à son apparition, jamais à chaque relevé.
const props = defineProps<{
  firstStart: FicheFirstStart;
  facts: FirstStartFacts;
}>();

const view = computed(() => firstStartView(props.firstStart, props.facts));
</script>

<template>
  <Callout
    v-if="view.kind === 'panel'"
    tone="info"
    role="status"
    :aria-label="t('fiche.firstStart.label')"
    data-first-start
  >
    <p v-for="(line, index) in view.lines" :key="index" class="first__line">{{ line }}</p>
  </Callout>
  <Callout v-else tone="info" role="status" data-first-start-unknown>
    {{ view.text }}
  </Callout>
</template>

<style scoped>
.first__line {
  margin: 0;
}
</style>
