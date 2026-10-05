<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";
import type { Level } from "@/link";
import HIcon from "../atoms/HIcon.vue";
import HTag from "../atoms/HTag.vue";

// Marque d'une mesure en alerte : pictogramme + libellé, jamais la couleur seule (la braise est
// proche de l'ambre d'attention). Une mesure normale n'affiche rien (BR-DASH-003).
const props = defineProps<{ level: Level }>();

const label = computed(() =>
  props.level === "critical" ? t("dash.levelCritical") : t("dash.levelAttention"),
);
</script>

<template>
  <HTag v-if="level !== 'normal'" :tone="level === 'critical' ? 'crit' : 'warn'" :data-level="level">
    <span class="badge"><HIcon name="alert" size="sm" />{{ label }}</span>
  </HTag>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}
</style>
