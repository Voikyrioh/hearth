<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatPercent, formatTemperature } from "@/dashboard/format";
import { gpuLoad, percentOf } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import Gauge from "../molecules/Gauge.vue";
import StatRow from "../molecules/StatRow.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Une carte graphique : charge, mémoire vidéo, température. Une mesure illisible se dit « Non
// disponible » sans toucher aux autres : une carte sans température garde sa charge et sa mémoire
// vidéo (BR-DASH-007, 008).
const props = defineProps<{ entry: ServerMachine; index: number }>();

const store = useDashboardStore();
const gpu = computed(() => props.entry.latest?.sample.gpus[props.index] ?? null);
const levels = computed(() => props.entry.latest?.levels.gpus[props.index] ?? null);
const load = computed(() => gpu.value?.loadPercent ?? null);
const videoPercent = computed(() =>
  gpu.value?.memoryUsedBytes != null && gpu.value.memoryTotalBytes != null
    ? percentOf(gpu.value.memoryUsedBytes, gpu.value.memoryTotalBytes)
    : null,
);
const temp = computed(() => gpu.value?.tempC ?? null);
const points = useMachineSeries(props.entry, gpuLoad(props.index));
const covered = useCoverage(props.entry);
</script>

<template>
  <div class="gpu" :data-gpu="index">
    <p class="gpu__name">{{ entry.machine?.gpus[index]?.name ?? gpu?.name }}</p>
    <div class="gpu__gauges">
      <Gauge
        size="sm"
        :label="t('dash.gpuLoad')"
        :ratio="load === null ? null : load / 100"
        :value-text="formatPercent(load)"
      />
      <Gauge
        size="sm"
        :label="t('dash.gpuMemory')"
        :ratio="videoPercent === null ? null : videoPercent / 100"
        :value-text="formatPercent(videoPercent)"
        :level="levels?.memory ?? 'normal'"
      />
      <TimeSeriesChart
        class="gpu__chart"
        :series="[{ points, tone: 'ac' }]"
        :max="100"
        :label="t('dash.chartGpu')"
        :window="store.windowKey"
        :covered-ms="covered"
      />
    </div>
    <dl class="gpu__rows">
      <StatRow
        :label="t('dash.gpuTemp')"
        :value="formatTemperature(temp)"
        :level="levels?.temp ?? 'normal'"
        :muted="temp === null"
      />
    </dl>
  </div>
</template>

<style scoped>
.gpu {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.gpu__name {
  font-weight: var(--fw-semibold);
}

.gpu__gauges {
  display: flex;
  align-items: center;
  gap: var(--space-4);
}

.gpu__chart {
  flex: 1;
  min-width: 0;
}

.gpu__rows {
  margin: 0;
}
</style>
