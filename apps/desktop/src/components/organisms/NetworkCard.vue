<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatRate } from "@/dashboard/format";
import { netDown, netUp } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import DashCard from "../molecules/DashCard.vue";
import StatRow from "../molecules/StatRow.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Réseau : débit montant et descendant, en turquoise (le contrepoint froid de la braise). Un débit
// nul s'affiche « 0 o/s » ; un débit non calculable (« Non disponible ») n'est pas un zéro.
const props = defineProps<{ entry: ServerMachine }>();

const store = useDashboardStore();
const net = computed(() => props.entry.latest?.sample.net ?? null);
const up = useMachineSeries(props.entry, netUp);
const down = useMachineSeries(props.entry, netDown);
const covered = useCoverage(props.entry);
</script>

<template>
  <DashCard :title="t('dash.network')">
    <dl class="net__rows">
      <StatRow
        :label="t('dash.netUp')"
        :value="formatRate(net?.upBytesPerS ?? null)"
        :muted="net === null"
      />
      <StatRow
        :label="t('dash.netDown')"
        :value="formatRate(net?.downBytesPerS ?? null)"
        :muted="net === null"
      />
    </dl>
    <TimeSeriesChart
      :series="[
        { points: down, tone: 'cool' },
        { points: up, tone: 'ac' },
      ]"
      :max="null"
      :label="t('dash.chartNet')"
      :window="store.windowKey"
      :covered-ms="covered"
    />
  </DashCard>
</template>

<style scoped>
.net__rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
}
</style>
