<script setup lang="ts">
import { computed } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import { t } from "@/i18n";
import type { BackupModsCompare } from "@/link";
import { groupChanges } from "@/services/backupMods";

// La comparaison d'une sauvegarde avec les mods actuels (HRT-294, BR-MOD-036, maquette Sauvegardes) : les changements
// regroupés par genre (ajouté depuis, retiré depuis, version changée, activé depuis, désactivé depuis), ou « Les mods
// actuels sont les mêmes que dans cette sauvegarde. ». La comparaison est CALCULÉE PAR L'AGENT (le client ne compare
// rien) ; tant que la liste actuelle n'est pas lue en entier, elle le dit et se complète seule. Elle ne modifie rien.
const props = defineProps<{ compare: BackupModsCompare }>();
const groups = computed(() => groupChanges(props.compare.changes));
</script>

<template>
  <section class="bc" data-backup-compare>
    <Callout v-if="!compare.complete" tone="info" data-backup-compare-partial>
      {{ t(compare.unread === 1 ? "backups.mods.compare.partialOne" : "backups.mods.compare.partial", { n: compare.unread }) }}
    </Callout>
    <p v-if="compare.same && compare.complete" class="bc__same" data-backup-compare-same>
      {{ t("backups.mods.compare.same") }}
    </p>
    <div v-for="group in groups" :key="group.kind" class="bc__group" :data-backup-compare-group="group.kind">
      <h5 class="bc__title">{{ group.title }}</h5>
      <ul class="bc__list">
        <li v-for="line in group.lines" :key="line">{{ line }}</li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.bc {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.bc__same {
  margin: 0;
  color: var(--tx-secondary);
}

.bc__title {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.bc__list {
  margin: var(--space-1) 0 0;
  padding-left: var(--space-5);
}
</style>
