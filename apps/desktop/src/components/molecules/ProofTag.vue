<script setup lang="ts">
import { computed } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import { proofLabel } from "@/fiche/messages";
import type { ProofKind } from "@/fiche/proof";
import { t } from "@/i18n";

// D'où vient un chiffre de ressource d'une fiche (HRT-102, BR-FICHE-008, BR-FICHE-057) : « Mesuré par Hearth »,
// « Mesuré au repos », « Chiffre de l'éditeur » ou « Non vérifié ». Le sens est dans le TEXTE, jamais dans la couleur
// seule : chaque preuve a aussi son pictogramme. Un chiffre de l'éditeur peut offrir « Voir la source » (bouton qui
// émet `source` ; c'est la page qui demande à la coquille d'ouvrir l'adresse de la fiche, Hearth ne la contacte pas).
const props = defineProps<{
  proof: ProofKind;
  /** Propose « Voir la source » (le chiffre de l'éditeur a une adresse). */
  source?: boolean;
  /** Ce que le chiffre mesure, pour nommer le lien aux lecteurs d'écran (« Mémoire »). */
  subject?: string;
}>();

defineEmits<{ source: [] }>();

const ICONS = {
  measured: "check",
  measured_idle: "clock",
  publisher: "info",
  unmeasured: "question",
} as const;

const icon = computed(() => ICONS[props.proof]);
</script>

<template>
  <span class="proof" :data-proof="proof">
    <HTag tone="neutral">
      <HIcon :name="icon" size="sm" />
      {{ proofLabel(proof) }}
    </HTag>
    <button
      v-if="source"
      type="button"
      class="proof__source"
      data-proof-source
      :aria-label="t('fiche.seeSourceLabel', { resource: subject ?? '' })"
      @click="$emit('source')"
    >
      {{ t("fiche.seeSource") }}
      <HIcon name="external" size="sm" />
    </button>
  </span>
</template>

<style scoped>
.proof {
  display: inline-flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.proof__source {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 0;
  border: 0;
  background: none;
  color: var(--ac);
  font: inherit;
  font-size: var(--fs-small);
  text-decoration: underline;
  cursor: pointer;
}

.proof__source:focus-visible {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}
</style>
