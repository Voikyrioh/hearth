<script setup lang="ts">
import { computed } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import ProofTag from "@/components/molecules/ProofTag.vue";
import { coverage, minimumVerdicts } from "@/fiche/fit";
import { coverageText, minimumText } from "@/fiche/messages";
import { proofOf, proofUrl, proven } from "@/fiche/proof";
import { t } from "@/i18n";
import type { Evidence, FicheDetail } from "@/link";
import { type EngineCapacity, formatCpu, formatMemory } from "@/resources";
import { formatSize } from "@/serviceOps/messages";

// La carte « Ressources » de la fiche d'un jeu (HRT-102, BR-FICHE-008, 020, 021, 023) : un tableau processeur, mémoire et
// espace disque contre minimum, recommandé et « Ce serveur », chaque ressource avec sa PREUVE (« Mesuré par Hearth »,
// « Mesuré au repos », « Chiffre de l'éditeur », « Non vérifié »). Une valeur non vérifiée n'a AUCUN nombre : la cellule
// dit « Non vérifié », jamais 0 ni un tiret. Sous le tableau, les constats en mots avec leur pictogramme : machine
// trop petite pour un minimum MESURÉ (erreur, « Installer » est désactivé par la page), chiffre de l'éditeur au-dessus
// de la machine (avertissement, rien n'est bloqué), minimum non vérifié (information), et, seulement quand la valeur
// recommandée est prouvée, si la machine la couvre. Aucune valeur n'est lue ailleurs que dans la fiche.
const props = defineProps<{
  detail: FicheDetail;
  /** Les bornes de la machine ; `null` : pas encore lues (la colonne « Ce serveur » le dit). */
  capacity: EngineCapacity | null;
}>();

const emit = defineEmits<{ source: [resource: "cpu" | "memory" | "disk"] }>();

type Row = {
  key: "cpu" | "memory" | "disk";
  label: string;
  min: string | null;
  recommended: string | null;
  server: string;
  evidence: Evidence;
};

const rows = computed<Row[]>(() => {
  const { cpu, memory, disk } = props.detail.resources;
  const cap = props.capacity;
  const cpuMin = proven(cpu.minMillis, cpu.evidence);
  const cpuRec = proven(cpu.recommendedMillis, cpu.evidence);
  const memMin = proven(memory.minBytes, memory.evidence);
  const memRec = proven(memory.recommendedBytes, memory.evidence);
  const diskNeed = proven(disk.installBytes, disk.evidence);
  const unread = t("fiche.thisServerUnread");
  return [
    {
      key: "cpu",
      label: t("fiche.resCpu"),
      min: cpuMin === null ? null : formatCpu(cpuMin),
      recommended: cpuRec === null ? null : formatCpu(cpuRec),
      server: cap ? formatCpu(cap.cpus * 1000) : unread,
      evidence: cpu.evidence,
    },
    {
      key: "memory",
      label: t("fiche.resMemory"),
      min: memMin === null ? null : formatMemory(memMin),
      recommended: memRec === null ? null : formatMemory(memRec),
      server: cap ? formatMemory(cap.memoryBytes) : unread,
      evidence: memory.evidence,
    },
    {
      key: "disk",
      label: t("fiche.resDisk"),
      min: diskNeed === null ? null : formatSize(diskNeed),
      recommended: null,
      server: t("fiche.diskAtInstall"),
      evidence: disk.evidence,
    },
  ];
});

const verdicts = computed(() => minimumVerdicts(props.detail, props.capacity));
const covers = computed(() => coverage(props.detail, props.capacity));
const TONES = { too_small: "error", publisher_above: "warn", unproven: "info" } as const;
</script>

<template>
  <DashCard :title="t('fiche.cardResources')" data-fiche-resources>
    <div class="res__scroll">
      <table class="res__table" :aria-label="t('fiche.resHead')">
        <thead>
          <tr>
            <th scope="col"><span class="sr-only">{{ t("fiche.cardResources") }}</span></th>
            <th scope="col">{{ t("fiche.colMin") }}</th>
            <th scope="col">{{ t("fiche.colRecommended") }}</th>
            <th scope="col">{{ t("fiche.colThisServer") }}</th>
            <th scope="col">{{ t("fiche.colProof") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.key" :data-res-row="row.key">
            <th scope="row">{{ row.label }}</th>
            <td data-res-min>
              <template v-if="row.min !== null">{{ row.min }}</template>
              <span v-else class="res__unproven">{{ t("fiche.notVerified") }}</span>
            </td>
            <td data-res-recommended>
              <template v-if="row.recommended !== null">{{ row.recommended }}</template>
              <span v-else class="res__unproven">{{ row.key === "disk" ? t("fiche.notApplicable") : t("fiche.notVerified") }}</span>
            </td>
            <td data-res-server>{{ row.server }}</td>
            <td>
              <ProofTag
                :proof="proofOf(row.evidence)"
                :source="proofUrl(row.evidence) !== null"
                :subject="row.label"
                @source="emit('source', row.key)"
              />
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div class="res__verdicts" data-res-verdicts>
      <Callout
        v-for="verdict in verdicts"
        :key="`${verdict.kind}-${verdict.lack}`"
        :tone="TONES[verdict.kind]"
        :role="verdict.kind === 'too_small' ? 'alert' : 'status'"
        :data-res-verdict="verdict.kind"
      >
        {{ minimumText(verdict) }}
      </Callout>
      <p
        v-for="item in covers"
        :key="item.lack"
        class="res__cover"
        :data-res-cover="`${item.lack}-${item.state}`"
      >
        <HIcon :name="item.state === 'covers' ? 'check' : 'alert'" size="sm" />
        {{ coverageText(item) }}
      </p>
    </div>
  </DashCard>
</template>

<style scoped>
.res__scroll {
  overflow-x: auto;
}

.res__table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.res__table th,
.res__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
  vertical-align: top;
}

.res__table thead th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
}

.res__table tbody th {
  font-weight: var(--fw-medium);
}

.res__unproven {
  color: var(--tx-secondary);
}

.res__verdicts {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.res__cover {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
}
</style>
