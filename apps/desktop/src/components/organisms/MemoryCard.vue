<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatPercent, formatUsage } from "@/dashboard/format";
import { memoryPercent, percentOf } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import DashCard from "../molecules/DashCard.vue";
import Gauge from "../molecules/Gauge.vue";
import StatRow from "../molecules/StatRow.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Mémoire : jauge d'occupation, « Utilisée / Totale », courbe.
const props = defineProps<{ entry: ServerMachine }>();

const store = useDashboardStore();
const mem = computed(() => props.entry.latest?.sample.mem ?? null);
const percent = computed(() =>
  mem.value ? percentOf(mem.value.usedBytes, mem.value.totalBytes) : null,
);
const level = computed(() => props.entry.latest?.levels.mem ?? "normal");
const points = useMachineSeries(props.entry, memoryPercent);
const covered = useCoverage(props.entry);
</script>

<template>
  <DashCard :title="t('dash.memory')">
    <div class="mem">
      <Gauge
        :label="t('dash.memory')"
        :ratio="percent === null ? null : percent / 100"
        :value-text="formatPercent(percent)"
        :level="level"
      />
      <TimeSeriesChart
        class="mem__chart"
        :series="[{ points, tone: 'ac' }]"
        :max="100"
        :label="t('dash.chartWithValue', { label: t('dash.chartMemory'), value: formatPercent(percent) })"
        :window="store.windowKey"
        :covered-ms="covered"
      />
    </div>
    <dl class="mem__rows">
      <StatRow
        :label="t('dash.memoryUsage')"
        :value="formatUsage(mem?.usedBytes ?? null, mem?.totalBytes ?? null)"
      />
    </dl>
  </DashCard>
</template>

<style scoped>
.mem {
  display: flex;
  flex: 1 1 auto;
  align-items: stretch;
  gap: var(--space-4);
}

.mem__chart {
  flex: 1;
  min-width: 0;
}

.mem__rows {
  margin: 0;
}
</style>
