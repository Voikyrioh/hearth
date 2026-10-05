<script setup lang="ts">
import { computed, watch } from "vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import StaleSurface from "@/components/molecules/StaleSurface.vue";
import CpuCard from "@/components/organisms/CpuCard.vue";
import DisksCard from "@/components/organisms/DisksCard.vue";
import GpuCard from "@/components/organisms/GpuCard.vue";
import MachineCard from "@/components/organisms/MachineCard.vue";
import MemoryCard from "@/components/organisms/MemoryCard.vue";
import NetworkCard from "@/components/organisms/NetworkCard.vue";
import TemperaturesCard from "@/components/organisms/TemperaturesCard.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { WINDOW_KEYS, type WindowKey } from "@/dashboard/series";
import { type MessageKey, t } from "@/i18n";
import { useDashboardStore } from "@/stores/dashboard";

// Tableau de bord d'un serveur : sa machine en direct (BR-DASH-001, 002). Les mesures viennent du
// pont (`useDashboardStore`), les niveaux d'alerte sont décidés par la coquille. Quand le lien
// n'est pas « Connecté », les dernières valeurs restent, grisées et datées (`StaleSurface`,
// BR-DASH-009). Les deux rôles voient la même chose (BR-DASH-013).
const { server, isConnected, lastContactAt } = useCurrentServer();
const store = useDashboardStore();

watch(
  () => server.value?.id,
  (id) => {
    if (id) void store.follow(id);
  },
  { immediate: true },
);

const entry = computed(() => (server.value ? store.of(server.value.id) : undefined));
const hasMachine = computed(() => entry.value?.machine != null);
// Connecté sans identité reçue : la première rafale arrive (chargement). Hors lien et sans rien en
// mémoire : rien à montrer, on le dit.
const loading = computed(
  () => !hasMachine.value && (entry.value?.settled !== true || isConnected.value),
);
const failed = computed(() => entry.value?.failed === true);

const WINDOW_LABELS: Record<WindowKey, MessageKey> = {
  "1m": "dash.window1m",
  "5m": "dash.window5m",
  "1h": "dash.window1h",
};
const windowOptions = WINDOW_KEYS.map((key) => ({ value: key, label: t(WINDOW_LABELS[key]) }));
</script>

<template>
  <div v-if="loading" class="dash__loading" role="status" :aria-label="t('dash.loading')">
    <HSpinner />
    <span>{{ t("dash.loading") }}</span>
  </div>
  <EmptyState
    v-else-if="!hasMachine"
    :title="failed ? t('dash.readFailed') : t('dash.waitingTitle')"
    :text="failed ? '' : t('dash.waitingText')"
    heading="h2"
  />
  <StaleSurface v-else-if="entry" :stale="!isConnected" :last-contact-at="lastContactAt">
    <div class="dash">
      <div class="dash__bar">
        <HSegmented
          :model-value="store.windowKey"
          :options="windowOptions"
          :label="t('dash.windowLabel')"
          @update:model-value="store.setWindow"
        />
      </div>
      <div class="dash__grid">
        <div class="dash__machine"><MachineCard :entry="entry" /></div>
        <div class="dash__cpu"><CpuCard :entry="entry" /></div>
        <div class="dash__memory"><MemoryCard :entry="entry" /></div>
        <div class="dash__gpu"><GpuCard :entry="entry" /></div>
        <div class="dash__network"><NetworkCard :entry="entry" /></div>
        <div class="dash__side">
          <DisksCard :entry="entry" />
          <TemperaturesCard :entry="entry" />
        </div>
      </div>
    </div>
  </StaleSurface>
</template>

<style scoped>
.dash {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.dash__bar {
  display: flex;
  align-items: center;
  min-height: var(--control-sm);
}

/* Grille de 12 colonnes : Machine 3, Processeur 5, Mémoire 4 ; Carte graphique 5, Réseau 4,
   Disques et Températures 3. Sous 1200 px : 6 colonnes, puis 12. */
.dash__grid {
  display: grid;
  grid-template-columns: repeat(12, minmax(0, 1fr));
  gap: var(--card-gap);
  align-items: start;
}

.dash__machine {
  grid-column: span 3;
}

.dash__cpu {
  grid-column: span 5;
}

.dash__memory {
  grid-column: span 4;
}

.dash__gpu {
  grid-column: span 5;
}

.dash__network {
  grid-column: span 4;
}

.dash__side {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  grid-column: span 3;
}

@media (max-width: 1200px) {
  .dash__machine,
  .dash__cpu,
  .dash__memory,
  .dash__gpu,
  .dash__network,
  .dash__side {
    grid-column: span 6;
  }
}

@media (max-width: 800px) {
  .dash__machine,
  .dash__cpu,
  .dash__memory,
  .dash__gpu,
  .dash__network,
  .dash__side {
    grid-column: span 12;
  }
}

.dash__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  min-height: var(--soon-min-height);
  color: var(--tx2);
}
</style>
