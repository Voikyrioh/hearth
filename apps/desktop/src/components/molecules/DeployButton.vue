<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import type { DeployDoor } from "@/deploy/availability";
import { t } from "@/i18n";

// Le bouton « Déployer un service » de la section Services et de son état vide (HRT-69, BR-DEPLOY-001, 027,
// BR-GRANT-014). Il est ABSENT quand le moteur n'est pas utilisable (la section l'explique) ; sinon visible : actif,
// ou inactif avec sa raison. Faute du droit, la raison est dite une seule fois par `ServiceRightReason`
// (`describedBy`) ; faute de moteur prêt, elle est écrite sous le bouton, jamais en infobulle seule. Lien coupé :
// inactif, avec l'explication du lien (`needsLink`). C'est le seul bouton principal de la section.
const props = defineProps<{
  door: DeployDoor;
  /** L'`id` du texte qui dit pourquoi le droit manque (`ServiceRightReason`). */
  rightsReasonId?: string;
}>();

defineEmits<{ click: [] }>();

const disabled = computed(() => props.door.kind === "disabled");
const hint = computed(() =>
  props.door.kind === "disabled" ? (props.door.reason ?? undefined) : undefined,
);
const byRights = computed(() => props.door.kind === "disabled" && props.door.byRights);
</script>

<template>
  <HButton
    v-if="door.kind !== 'hidden'"
    data-deploy-open
    :needs-link="true"
    :disabled="disabled"
    :hint="hint"
    :reason-below="!byRights && hint !== undefined"
    :described-by="byRights ? rightsReasonId : undefined"
    @click="$emit('click')"
  >
    <HIcon name="plus" size="sm" />
    {{ t("deploy.open") }}
  </HButton>
</template>
