<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { SCREEN_ILLUSTRATIONS } from "@/assets/illustrations/screens";
import HButton from "@/components/atoms/HButton.vue";
import HSegmented from "@/components/atoms/HSegmented.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
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
// n'est pas « Connecté », les dernières valeurs restent, grisées et datées (`StaleSurface` du gabarit,
// BR-DASH-009). Les deux rôles voient la même chose (BR-DASH-013).
const { server, isConnected } = useCurrentServer();
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

/** FIX:01M4D1K9GV5X6MJTHDB8RYPMS6 — au-delà de ce délai, un chargement qui ne finit pas le dit et propose une action (revue UX C5). */
const SLOW_LOADING_MS = 3000;
const slow = ref(false);
let slowTimer: ReturnType<typeof setTimeout> | undefined;
watch(
  loading,
  (isLoading) => {
    clearTimeout(slowTimer);
    slow.value = false;
    if (isLoading) slowTimer = setTimeout(() => (slow.value = true), SLOW_LOADING_MS);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(slowTimer));

/** Reprend l'abonnement aux mesures du serveur courant. */
function retryLoading() {
  const id = server.value?.id;
  if (!id) return;
  store.forget(id);
  slow.value = false;
  void store.follow(id);
}

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
    <span>{{ slow ? t("dash.loadingSlow") : t("dash.loading") }}</span>
    <HButton v-if="slow" variant="secondary" data-dash-retry @click="retryLoading">
      {{ t("dash.retryLoading") }}
    </HButton>
  </div>
  <EmptyState
    v-else-if="!hasMachine"
    :title="failed ? t('dash.readFailed') : t('dash.waitingTitle')"
    :text="failed ? '' : t('dash.waitingText')"
    heading="h2"
    :illustration="failed || isConnected ? undefined : (SCREEN_ILLUSTRATIONS.offline ?? undefined)"
  />
  <template v-else-if="entry">
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
        <div class="dash__disks"><DisksCard :entry="entry" /></div>
        <div class="dash__temps"><TemperaturesCard :entry="entry" /></div>
      </div>
    </div>
  </template>
</template>

<style scoped>
/* FIX:01M4D1K5GKA4XPJH8391E1HWKZ — le conteneur de la grille : les colonnes suivent la largeur de la PAGE (pas celle de la fenêtre), car la barre
   des serveurs et la navigation en retirent plus de 270 px (HRT-34 : à 1280 px les courbes étaient écrasées). */
.dash {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  container: dash / inline-size;
}

.dash__bar {
  display: flex;
  align-items: center;
  min-height: var(--control-sm);
}

/* Grille de 12 colonnes : Machine 3, Processeur 5, Mémoire 4 ; Carte graphique 4, Réseau 3, Disques 3,
   Températures 2 (HRT-34 : sinon Disques + Températures empilés dépassaient la moitié basse à 1920 px).
   Ancienne répartition : Carte graphique 5, Réseau 4,
   Disques 3 ; Températures sous Mémoire (HRT-34 : Disques et Températures empilés dépassaient la moitié basse de
   l'écran à 1920 px, l'essentiel ne tenait pas sans défiler). Une rangée = une hauteur : les cartes d'une rangée s'étirent. Sous 1300 px de
   page : 6 colonnes, puis 1 sous 700 px (voir plus bas). */
.dash__grid {
  display: grid;
  grid-template-columns: repeat(12, minmax(0, 1fr));
  gap: var(--card-gap);
  align-items: stretch;
}

/* Chaque case est une colonne qui donne toute sa hauteur à sa carte (jamais de trou sous une carte). */
.dash__machine,
.dash__cpu,
.dash__memory,
.dash__gpu,
.dash__network,
.dash__disks,
.dash__temps {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  min-width: 0;
}

.dash__machine > *,
.dash__cpu > *,
.dash__memory > *,
.dash__gpu > *,
.dash__network > *,
.dash__disks > *,
.dash__temps > * {
  flex: 1 1 auto;
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
  grid-column: span 4;
}

.dash__network {
  grid-column: span 3;
}

.dash__disks {
  grid-column: span 3;
}

.dash__temps {
  grid-column: span 2;
}

/* Page de moins de 1300 px (fenêtre ouverte à 1280, 1366) : 6 colonnes, deux cartes par rangée, la carte
   graphique (trois jauges et sa courbe) sur toute la largeur ; Disques sur toute la largeur. */
@container dash (max-width: 1300px) {
  .dash__grid {
    grid-template-columns: repeat(6, minmax(0, 1fr));
  }

  .dash__machine {
    order: 1;
    grid-column: span 3;
  }

  .dash__cpu {
    order: 2;
    grid-column: span 3;
  }

  .dash__memory {
    order: 3;
    grid-column: span 3;
  }

  .dash__network {
    order: 4;
    grid-column: span 3;
  }

  .dash__gpu {
    order: 5;
    grid-column: span 6;
  }

  .dash__disks {
    order: 6;
    grid-column: span 3;
  }

  .dash__temps {
    order: 7;
    grid-column: span 3;
  }
}

/* Page étroite (fenêtre minimale 1 100 px, ou 800 px de page) : une colonne, rien ne se chevauche. */
@container dash (max-width: 700px) {
  .dash__grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .dash__machine,
  .dash__cpu,
  .dash__memory,
  .dash__gpu,
  .dash__network,
  .dash__disks,
  .dash__temps {
    grid-column: auto;
  }
}

.dash__loading {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  min-height: var(--soon-min-height);
  color: var(--tx2);
}
</style>
