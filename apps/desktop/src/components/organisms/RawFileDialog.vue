<script setup lang="ts">
import { ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import Callout from "@/components/molecules/Callout.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { t } from "@/i18n";

// « Modifier le fichier brut » (HRT-109, BR-FICHE-046) : le fichier de réglages reste accessible, c'est le « sur mesure »
// permis. Le bandeau dit, mot pour mot, que Hearth ne vérifie pas ce qu'on écrit ; la case « Je comprends » se coche une
// fois par ouverture. L'éditeur de fichiers de l'onglet « Fichiers » n'existe pas encore : « Ouvrir le fichier » est
// présent mais inactif, avec sa phrase, et la voie de remplacement est l'import par collage. Un jeu qui réécrit son
// fichier à chaque démarrage n'a pas ce bouton du tout (l'onglet le dit).
const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();

const understood = ref(false);
watch(
  () => props.open,
  () => {
    understood.value = false;
  },
);
</script>

<template>
  <FormDialog
    :open="open"
    :title="t('gameSettings.rawTitle')"
    hide-submit
    :cancel-label="t('gameSettings.rawClose')"
    @cancel="emit('close')"
  >
    <div class="raw" data-raw-dialog>
      <Callout tone="warn" data-raw-warning>{{ t("gameSettings.rawWarning") }}</Callout>
      <HCheckbox v-model="understood" :label="t('gameSettings.rawUnderstand')" data-raw-understand />
      <div class="raw__open">
        <HButton variant="secondary" disabled data-raw-open>{{ t("gameSettings.rawOpen") }}</HButton>
        <p class="raw__soon" data-raw-soon>{{ t("gameSettings.rawSoon") }}</p>
      </div>
    </div>
  </FormDialog>
</template>

<style scoped>
.raw {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.raw__open {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
}

.raw__soon {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
