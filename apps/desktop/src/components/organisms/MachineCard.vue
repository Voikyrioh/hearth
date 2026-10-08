<script setup lang="ts">
import { computed } from "vue";
import { formatFrequency, formatGb, formatUptime } from "@/dashboard/format";
import { t } from "@/i18n";
import type { ServerMachine } from "@/stores/dashboard";
import HMiddleText from "../atoms/HMiddleText.vue";
import DashCard from "../molecules/DashCard.vue";
import StatRow from "../molecules/StatRow.vue";

// Identité de la machine (nom, système, processeur, mémoire, disques) et durée de fonctionnement.
// La liste des disques suit celle de l'échantillon courant : un disque monté ou retiré s'y reflète
// (BR-DASH-012).
const props = defineProps<{ entry: ServerMachine }>();

const machine = computed(() => props.entry.machine);
const system = computed(() => {
  const os = machine.value?.os;
  if (!os) return t("dash.unavailable");
  return os.version ? t("dash.systemLine", { name: os.name, version: os.version }) : os.name;
});
const processor = computed(() => {
  const cpu = machine.value?.cpu;
  if (!cpu) return t("dash.unavailable");
  const threads = t("dash.cores", { n: cpu.logicalCores });
  const line = t("dash.processorLine", { model: cpu.model, threads });
  return cpu.frequencyMhz
    ? t("dash.processorFreq", { line, freq: formatFrequency(cpu.frequencyMhz) })
    : line;
});
const disks = computed(() => {
  const live = props.entry.latest?.sample.disks;
  const listed = live ?? machine.value?.disks ?? [];
  return listed.map((disk) => ({
    key: `${disk.name}|${disk.mount}`,
    mount: disk.mount,
    size: formatGb(disk.totalBytes),
  }));
});
const uptime = computed(() => formatUptime(props.entry.latest?.sample.uptimeS ?? null));
</script>

<template>
  <div class="machine">
    <DashCard :title="t('dash.machine')">
      <dl class="machine__rows">
        <StatRow stacked :label="t('dash.name')" :value="machine?.name ?? t('dash.unavailable')" />
        <StatRow stacked :label="t('dash.system')" :value="system" />
        <StatRow stacked :label="t('dash.processor')" :value="processor" />
        <StatRow
          stacked
          :label="t('dash.memory')"
          :value="formatGb(machine?.memoryTotalBytes ?? null)"
        />
        <StatRow stacked :label="t('dash.disks')" :value="t('dash.unavailable')">
          <span v-if="disks.length === 0">{{ t("dash.unavailable") }}</span>
          <!-- FIX:01M4EPX88BTXFX1PX7E57WGK38 : chaque disque (point de montage tronqué au milieu, taille) tient dans la carte. -->
          <span v-for="disk in disks" :key="disk.key" class="machine__disk">
            <HMiddleText class="machine__mount" :text="disk.mount" /><span class="machine__size">{{ disk.size }}</span>
          </span>
        </StatRow>
      </dl>
    </DashCard>
    <DashCard :title="t('dash.uptime')">
      <p class="machine__uptime">{{ uptime }}</p>
    </DashCard>
  </div>
</template>

<style scoped>
.machine {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  min-width: 0;
}

.machine > :first-child {
  flex: 1;
}

.machine__disk {
  display: flex;
  flex: 0 0 100%;
  gap: var(--space-2);
  max-width: 100%;
  min-width: 0;
}

.machine__mount {
  flex: 0 1 auto;
  min-width: 0;
}

.machine__size {
  flex: none;
}

.machine__rows {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
}

.machine__uptime {
  font-family: var(--font-mono);
  font-size: var(--fs-h3);
  font-variant-numeric: tabular-nums;
}
</style>
