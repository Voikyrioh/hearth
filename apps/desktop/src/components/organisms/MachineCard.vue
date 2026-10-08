<script setup lang="ts">
import { computed } from "vue";
import { formatFrequency, formatGb, formatUptime } from "@/dashboard/format";
import { t } from "@/i18n";
import type { ServerMachine } from "@/stores/dashboard";
import HMiddleText from "../atoms/HMiddleText.vue";
import DashCard from "../molecules/DashCard.vue";

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
// Le processeur sur deux lignes : le modèle, puis les cœurs et la fréquence (HRT-47, S5).
const processorModel = computed(() => machine.value?.cpu.model ?? t("dash.unavailable"));
const processorDetail = computed(() => {
  const cpu = machine.value?.cpu;
  if (!cpu) return "";
  const cores = t("dash.cores", { n: cpu.logicalCores });
  return cpu.frequencyMhz ? `${cores} · ${formatFrequency(cpu.frequencyMhz)}` : cores;
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
        <dt class="machine__label">{{ t("dash.name") }}</dt>
        <dd class="machine__value">{{ machine?.name ?? t("dash.unavailable") }}</dd>
        <dt class="machine__label">{{ t("dash.system") }}</dt>
        <dd class="machine__value">{{ system }}</dd>
        <dt class="machine__label">{{ t("dash.processor") }}</dt>
        <dd class="machine__value">
          <span data-machine-cpu-model>{{ processorModel }}</span>
          <span v-if="processorDetail" class="machine__detail" data-machine-cpu-detail>{{ processorDetail }}</span>
        </dd>
        <dt class="machine__label">{{ t("dash.memory") }}</dt>
        <dd class="machine__value">{{ formatGb(machine?.memoryTotalBytes ?? null) }}</dd>
        <dt class="machine__label">{{ t("dash.disks") }}</dt>
        <dd class="machine__value">
          <span v-if="disks.length === 0">{{ t("dash.unavailable") }}</span>
          <!-- FIX:01M4EPX88BTXFX1PX7E57WGK38 : une liste, un rang par disque (point de montage tronqué au milieu, puis taille alignée à droite). -->
          <ul v-else class="machine__disks">
            <li v-for="disk in disks" :key="disk.key" class="machine__disk" data-machine-disk>
              <HMiddleText class="machine__mount" :text="disk.mount" />
              <span class="machine__size">{{ disk.size }}</span>
            </li>
          </ul>
        </dd>
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

/* Grille à deux colonnes : libellés à gauche, valeurs à droite (proposition S5, dans le design system : jetons seulement). */
.machine__rows {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: var(--space-2) var(--space-4);
  align-items: baseline;
  margin: 0;
}

.machine__label {
  color: var(--tx2);
}

.machine__value {
  display: flex;
  flex-direction: column;
  min-width: 0;
  margin: 0;
  /* Une valeur longue d'un seul tenant (nom de machine, modèle de processeur) se coupe proprement dans sa colonne. */
  overflow-wrap: anywhere;
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.machine__detail {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.machine__disks {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}

.machine__disk {
  display: flex;
  justify-content: space-between;
  gap: var(--space-3);
  min-width: 0;
}

.machine__mount {
  flex: 0 1 auto;
  min-width: 0;
}

.machine__size {
  flex: none;
  color: var(--tx2);
}

.machine__uptime {
  font-family: var(--font-mono);
  font-size: var(--fs-h3);
  font-variant-numeric: tabular-nums;
}
</style>
