<script setup lang="ts">
import { computed } from "vue";
import { useCoverage, useMachineSeries } from "@/composables/useMachineSeries";
import { formatGb, formatPercent, formatUsage } from "@/dashboard/format";
import { fullestDiskPercent, percentOf } from "@/dashboard/series";
import { t } from "@/i18n";
import { type ServerMachine, useDashboardStore } from "@/stores/dashboard";
import HMeter from "../atoms/HMeter.vue";
import DashCard from "../molecules/DashCard.vue";
import LevelBadge from "../molecules/LevelBadge.vue";
import StatRow from "../molecules/StatRow.vue";
import TimeSeriesChart from "../molecules/TimeSeriesChart.vue";

// Disques : la liste suit les montages de chaque échantillon, un disque monté ou retiré apparaît
// ou disparaît sans rechargement (BR-DASH-012). Chaque disque : occupation, « Utilisé / Total »,
// « Libre : X Go », couleur et marque selon son niveau (BR-DASH-003).
const props = defineProps<{ entry: ServerMachine }>();

const store = useDashboardStore();
const disks = computed(() => {
  const latest = props.entry.latest;
  if (!latest) return [];
  return latest.sample.disks.map((disk, index) => {
    const percent = percentOf(disk.usedBytes, disk.totalBytes);
    return {
      key: `${disk.name}|${disk.mount}`,
      name: disk.name,
      mount: disk.mount,
      percent,
      level: latest.levels.disks[index] ?? "normal",
      usage: formatUsage(disk.usedBytes, disk.totalBytes),
      free: formatGb(Math.max(0, disk.totalBytes - disk.usedBytes)),
    };
  });
});
const points = useMachineSeries(props.entry, fullestDiskPercent);
const covered = useCoverage(props.entry);
</script>

<template>
  <DashCard :title="t('dash.disks')">
    <p v-if="disks.length === 0" class="disks__none">{{ t("dash.unavailable") }}</p>
    <ul v-else class="disks__list">
      <li v-for="disk in disks" :key="disk.key" class="disk" :data-level="disk.level">
        <div class="disk__head">
          <span class="disk__name">{{ disk.name }}</span>
          <span v-if="disk.mount !== disk.name" class="disk__mount">{{ disk.mount }}</span>
          <span class="disk__gap" />
          <span class="disk__percent">{{ formatPercent(disk.percent) }}</span>
          <LevelBadge :level="disk.level" />
        </div>
        <HMeter
          :ratio="disk.percent === null ? null : disk.percent / 100"
          :level="disk.level"
          :label="t('dash.gaugeLabel', { name: disk.name, value: formatPercent(disk.percent) })"
        />
        <dl class="disk__rows">
          <StatRow stacked :label="t('dash.diskUsage')" :value="disk.usage" />
        </dl>
        <p class="disk__free">{{ t("dash.diskFree", { size: disk.free }) }}</p>
      </li>
    </ul>
    <TimeSeriesChart
      :series="[{ points, tone: 'ac' }]"
      :max="100"
      :label="t('dash.chartDisk')"
      :window="store.windowKey"
      :covered-ms="covered"
    />
  </DashCard>
</template>

<style scoped>
.disks__none {
  color: var(--tx2);
}

.disks__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  margin: 0;
  padding: 0;
  list-style: none;
}

.disk {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.disk__head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-width: 0;
}

.disk__name {
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.disk__gap {
  flex: 1;
}

.disk__percent {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.disk__mount {
  /* FIX:01M4D1K6V5DS7DQVR6ZV7A3HGY */
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.disk__free {
  color: var(--tx3);
  font-size: var(--fs-small);
}

.disk__rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
}
</style>
