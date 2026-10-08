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
const summary = computed(() => titles.value.join(", "));
</script>

<template>
  <div class="cores">
    <HBars :values="cores" :titles="titles" :label="`${label} : ${summary}`" />
    <p class="cores__ends">
      <span>{{ t("dash.coreFirst") }}</span>
      <span>{{ t("dash.core", { n: cores.length }) }}</span>
    </p>
  </div>
</template>

<style scoped>
.cores {
  display: flex;
  flex: 2 1 auto;
  flex-direction: column;
  gap: var(--space-1);
}

.cores__ends {
  display: flex;
  justify-content: space-between;
  color: var(--tx3);
  font-size: var(--fs-small);
}
</style>
