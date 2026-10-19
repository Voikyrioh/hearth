<script setup lang="ts">
import { computed } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { LIFECYCLE_ACT_KINDS, type ServiceInfo } from "@/link";
import {
  effectText,
  type Gesture,
  nameOf,
  refusalText,
  staleNotice,
  submitLabel,
  titleOf,
  warningsOf,
} from "@/services/lifecycle";

// La fenêtre de démarrage, d'arrêt et de redémarrage d'un service (HRT-65, BR-LIFE-007, 008, 016, 017, 022) : UNE
// fenêtre pour l'effet ET la confirmation du socle (`AdminActDialog`, qui ajoute « Votre mot de passe » selon ce que
// l'agent annonce). Arrêter et redémarrer disent toujours ce qui va se passer, avec le délai d'arrêt RÉEL du service
// (jamais une constante de l'interface), puis les avertissements propres au service : créé hors de Hearth, règle
// « Redémarre toujours », service géré par le système du serveur. Démarrer n'a pas d'effet à confirmer : seul le champ
// du socle. Fermer (Annuler, Échap) ne change rien et n'envoie rien (BR-LIFE-017). Si le service change sous la
// fenêtre (BR-LIFE-022) elle le dit et le bouton de confirmation devient inactif ; « Annuler » reste actif.
const props = defineProps<{
  open: boolean;
  serverId: string;
  gesture: Gesture;
  /** Le service tel qu'il était à l'ouverture (nom, avertissements). */
  service: ServiceInfo;
  /** Le service tel qu'il est maintenant dans la liste ; `null` : il a disparu. */
  current: ServiceInfo | null;
  /** Une action tient le service depuis l'ouverture de la fenêtre. */
  locked: boolean;
  perform: (adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const kind = computed(() => LIFECYCLE_ACT_KINDS[props.gesture]);
const shown = computed(() => props.current ?? props.service);
const name = computed(() => nameOf(props.service));
const stale = computed(() => staleNotice(props.gesture, props.current, props.locked));
const effect = computed(() =>
  props.gesture === "start" ? null : effectText(props.gesture, shown.value),
);
const warnings = computed(() => (props.gesture === "start" ? [] : warningsOf(shown.value)));

function refusal(refused: { kind: string }): string {
  return refusalText(refused, props.service);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="kind"
    :title="titleOf(gesture, name)"
    :submit-label="submitLabel(gesture)"
    :can-submit="stale === null"
    :perform="perform"
    :refusal-text="refusal"
    data-lifecycle-dialog
    @close="emit('close')"
  >
    <Callout v-if="stale" tone="info" role="status" data-lifecycle-stale>{{ stale }}</Callout>
    <Callout v-if="effect" tone="warn" data-lifecycle-effect>{{ effect }}</Callout>
    <Callout
      v-for="warning in warnings"
      :key="warning.id"
      :tone="warning.tone"
      :data-lifecycle-warning="warning.id"
    >
      {{ warning.text }}
    </Callout>
  </AdminActDialog>
</template>
