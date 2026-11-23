<script setup lang="ts">
import { computed } from "vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import { t } from "@/i18n";
import type { BackupModsCompare, ChangeKind } from "@/link";
import { groupChanges } from "@/services/backupMods";

// La comparaison d'une sauvegarde avec les mods actuels (HRT-294, BR-MOD-036, maquette Sauvegardes) : les changements
// regroupés par genre (ajouté depuis, retiré depuis, version changée, activé depuis, désactivé depuis), ou « Les mods
// actuels sont les mêmes que dans cette sauvegarde. ». La comparaison est CALCULÉE PAR L'AGENT (le client ne compare
// rien) ; tant que la liste actuelle n'est pas lue en entier, elle le dit et se complète seule. Elle ne modifie rien.
const props = defineProps<{ compare: BackupModsCompare }>();
const TONES: Record<ChangeKind, "ok" | "warn" | "crit" | "neutral"> = {
  added: "ok",
  removed: "crit",
  version_changed: "warn",
  enabled: "ok",
  disabled: "neutral",
  unknown: "neutral",
};
const groups = computed(() => groupChanges(props.compare.changes));
/** Les changements dits à l'écran : le compte de l'introduction est celui des lignes qu'on voit. */
const shown = computed(() => groups.value.reduce((sum, group) => sum + group.lines.length, 0));
</script>

<template>
  <section class="bc" data-backup-compare>
    <h4 class="bc__heading" data-backup-compare-title>{{ t("backups.mods.compare.title") }}</h4>
    <Callout v-if="!compare.complete" tone="info" data-backup-compare-partial>
      {{ t(compare.unread === 1 ? "backups.mods.compare.partialOne" : "backups.mods.compare.partial", { n: compare.unread }) }}
    </Callout>
    <p v-if="compare.same && compare.complete" class="bc__same" data-backup-compare-same>
      {{ t("backups.mods.compare.same") }}
    </p>
    <p v-else-if="compare.complete" class="bc__intro" data-backup-compare-intro>
      {{ t(shown === 1 ? "backups.mods.compare.introOne" : "backups.mods.compare.intro", { n: shown }) }}
    </p>
    <div v-for="group in groups" :key="group.kind" class="bc__group" :data-backup-compare-group="group.kind">
      <p v-for="line in group.lines" :key="line" class="bc__row">
        <HTag :tone="TONES[group.kind]">{{ group.title }}</HTag>
        <span class="bc__line">{{ line }}</span>
      </p>
    </div>
    <p class="bc__prepared" data-backup-compare-prepared>{{ t("backups.mods.compare.prepared") }}</p>
  </section>
</template>

<style scoped>
.bc {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
}

.bc__heading {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.bc__same,
.bc__intro,
.bc__prepared {
  margin: 0;
  color: var(--tx-secondary);
}

.bc__prepared {
  font-size: var(--fs-small);
}

.bc__group {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.bc__row {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  align-items: baseline;
  gap: var(--space-3);
  margin: 0;
}
</style>
