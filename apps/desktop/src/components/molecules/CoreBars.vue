<script setup lang="ts">
import { computed } from "vue";
import { formatPercent } from "@/dashboard/format";
import { t } from "@/i18n";
import HBars from "../atoms/HBars.vue";

// Une barre par cœur logique (« Cœur 1 », « Cœur 2 »…), son pourcentage en infobulle. Le nombre
// de barres suit le nombre de cœurs de chaque échantillon.
const props = defineProps<{ cores: readonly number[]; label: string }>();

const titles = computed(() =>
  props.cores.map((core, index) => `${t("dash.core", { n: index + 1 })} : ${formatPercent(core)}`),
);
</script>

<template>
  <HBars :values="cores" :titles="titles" :label="label" />
</template>
