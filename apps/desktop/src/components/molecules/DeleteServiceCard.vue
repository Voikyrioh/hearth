<script setup lang="ts">
import { computed, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { t } from "@/i18n";
import type { DeleteDoor } from "@/services/delete";
import type { RightsGate } from "@/services/rights";

// La carte « Suppression » du bas du détail d'un service (HRT-86, BR-DEL-002, 003, 026) : « Supprimer ce service »,
// séparé des actions de pilotage, et nulle part ailleurs (jamais dans le menu de la ligne). Un service créé par Hearth
// montre le bouton, actif ou désactivé avec sa raison écrite dessous (le droit par `ServiceRightReason`, une action en
// cours, un état que Hearth ne pilote pas, le moteur pas prêt ; jamais une infobulle seule). Un service adopté ne porte
// aucun bouton : la carte dit que Hearth ne supprime jamais un service créé hors de Hearth. Un service en déploiement
// n'a pas cette carte (le détail réduit, BR-SVC-025) : la porte `hidden` ne l'affiche pas.
const props = defineProps<{
  door: DeleteDoor;
  gate: RightsGate;
  /** L'`id` de la raison du droit que le détail écrit déjà, une seule fois (BR-GRANT-014) : le bouton la décrit. */
  rightsReasonId: string;
}>();
const emit = defineEmits<{ delete: [] }>();

const reasonId = useId();
const blocked = computed(() => (props.door.kind === "blocked" ? props.door : null));
const rightsText = computed(
  () =>
    blocked.value?.rights === true && ["denied", "old-agent", "failed"].includes(props.gate.kind),
);
const reason = computed(() => {
  const own = blocked.value;
  if (!own || own.rights || own.reason === null) return null;
  return own.by === null ? own.reason : `${own.reason} ${own.by}`;
});
const describedBy = computed(() => {
  if (rightsText.value) return props.rightsReasonId;
  return reason.value ? reasonId : undefined;
});
</script>

<template>
  <DashCard v-if="door.kind !== 'hidden'" :title="t('serviceDelete.cardTitle')" data-detail-delete>
    <Callout v-if="door.kind === 'adopted'" tone="info" inline data-delete-adopted>
      {{ t("serviceDelete.adoptedNote") }}
    </Callout>
    <div v-else class="card">
      <HButton
        variant="danger"
        needs-link
        :disabled="door.kind === 'blocked'"
        :described-by="describedBy"
        data-delete-open
        @click="emit('delete')"
      >
        {{ t("serviceDelete.button") }}
      </HButton>
      <p v-if="reason" :id="reasonId" class="card__reason" data-delete-reason>{{ reason }}</p>
    </div>
  </DashCard>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  margin-top: var(--space-4);
}

.card__reason {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}
</style>
