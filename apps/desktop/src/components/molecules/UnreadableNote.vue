<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";

// FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB (BR-COMPAT-001 point 10) : une liste dont l'agent a envoyé des éléments que cette
// version ne sait pas lire est PARTIELLE, et le dit : une ligne discrète sous la liste, au singulier ou au pluriel.
// Rien n'est montré quand tout est lisible. Ne montre jamais le contenu d'un élément illisible, seulement leur nombre.
const props = defineProps<{ count: number }>();

const text = computed(() =>
  props.count === 1 ? t("common.unreadableOne") : t("common.unreadableMany", { n: props.count }),
);
</script>

<template>
  <p v-if="count > 0" class="unreadable" role="status" data-unreadable>{{ text }}</p>
</template>

<style scoped>
.unreadable {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
