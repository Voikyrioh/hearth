<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatTemperature } from "@/dashboard/format";
import { hottest } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import DashCard from "../molecules/DashCard.vue";
import StatRow from "../molecules/StatRow.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Températures : une ligne par sonde (marque d'alerte à 80 et 90 °C, décidée par la coquille) et la
// courbe de la plus haute. Sans sonde, la section reste avec son explication (BR-DASH-006).
const props = defineProps<{ entry: ServerMachine }>();

const store = useDashboardStore();
const present = computed(() => props.entry.machine?.capabilities.temps === true);
const probes = computed(() => {
  const latest = props.entry.latest;
  if (!latest) return [];
  return latest.sample.temps.map((temp, index) => ({
    label: temp.label,
    celsius: temp.celsius,
    level: latest.levels.temps[index] ?? "normal",
  }));
});
const points = useMachineSeries(props.entry, hottest);
const hottestNow = computed(() => (props.entry.latest ? hottest(props.entry.latest.sample) : null));
const covered = useCoverage(props.entry);
</script>

<template>
  <DashCard :title="t('dash.temperatures')">
    <p v-if="!present" class="temps__none">{{ t("dash.noProbes") }}</p>
    <template v-else>
      <p v-if="probes.length === 0" class="temps__none">{{ t("dash.unavailable") }}</p>
      <dl v-else class="temps__rows">
        <StatRow
          v-for="(probe, index) in probes"
          :key="index"
          :label="probe.label"
          :value="formatTemperature(probe.celsius)"
          :level="probe.level"
        />
      </dl>
      <TimeSeriesChart
        :series="[{ points, tone: 'ac' }]"
        :max="null"
        :at-least="100"
        :label="t('dash.chartWithValue', { label: t('dash.chartTemp'), value: formatTemperature(hottestNow) })"
        :window="store.windowKey"
        :covered-ms="covered"
      />
    </template>
  </DashCard>
</template>

<style scoped>
.temps__none {
  color: var(--tx2);
}

.temps__rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
}
</style>
