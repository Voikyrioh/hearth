<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatPercent } from "@/dashboard/format";
import { cpuLoad } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import CoreBars from "../molecules/CoreBars.vue";
import DashCard from "../molecules/DashCard.vue";
import Gauge from "../molecules/Gauge.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Processeur : charge globale (jauge, courbe) et une barre par cœur (BR-DASH-001). Le niveau de
// la jauge est celui que la coquille a décidé : « tenu 30 s » (BR-DASH-004).
const props = defineProps<{ entry: ServerMachine }>();

const store = useDashboardStore();
const cpu = computed(() => props.entry.latest?.sample.cpu ?? null);
const level = computed(() => props.entry.latest?.levels.cpu ?? "normal");
const cores = computed(() => props.entry.latest?.sample.cores ?? []);
const points = useMachineSeries(props.entry, cpuLoad);
const covered = useCoverage(props.entry);
</script>

<template>
  <DashCard :title="t('dash.processor')">
    <div class="cpu">
      <Gauge
        :label="t('dash.cpuGlobal')"
        :ratio="cpu === null ? null : cpu / 100"
        :value-text="formatPercent(cpu)"
        :level="level"
      />
      <TimeSeriesChart
        class="cpu__chart"
        :series="[{ points, tone: 'ac' }]"
        :max="100"
        :label="t('dash.chartWithValue', { label: t('dash.chartCpu'), value: formatPercent(cpu) })"
        :window="store.windowKey"
        :covered-ms="covered"
      />
    </div>
    <CoreBars :cores="cores" :label="t('dash.coresLabel')" />
  </DashCard>
</template>

<style scoped>
.cpu {
  display: flex;
  flex: 3 1 auto;
  align-items: stretch;
  gap: var(--space-4);
}

.cpu__chart {
  flex: 1;
  min-width: 0;
}

.cpu :deep(.gauge) {
  align-self: center;
}
</style>
