<script setup lang="ts">
import { computed } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import ResourceSlider from "@/components/molecules/ResourceSlider.vue";
import { t } from "@/i18n";
import {
  cpuStops,
  type EngineCapacity,
  formatMemory,
  memoryStops,
  proposedMemoryBytes,
  type Resources,
} from "@/resources";

// Mémoire et processeur d'un service qu'on crée (HRT-146). Les bornes viennent de la machine (`capacity`, lue sur
// `GET /engine/capacity`) ; la valeur de départ est donnée par l'appelant (`startingResources`). Il n'existe aucune
// position « sans limite » (BR-RES-037) : une valeur vide ou refusée est signalée sur place et vaut `null`. Le
// composant ne juge pas l'envoi : l'appelant (déploiement, installation par fiche), qui a déjà `capacity` et la
// valeur, garde son bouton d'envoi inactif tant que `resourcesIssue(capacity, valeur)` n'est pas `null`.
const props = withDefaults(
  defineProps<{
    /** Les bornes de la machine ; `null` tant que le serveur n'a pas répondu. */
    capacity: EngineCapacity | null;
    modelValue: Resources;
    /** La mémoire recommandée d'une fiche et sa preuve ; `null` pour une application sans fiche. */
    recommended?: { bytes: number; proof: "measured" | "measured_idle" | "publisher" } | null;
    /**
     * Un texte à la place de « Hearth ne connaît pas cette application et propose… » quand `recommended` est `null` : une fiche
     * dont la mémoire n'est pas prouvée le dit avec ses mots (HRT-102). Sans lui, la proposition de BR-RES-039.
     */
    memoryNote?: string | null;
    /** Une ligne sous le curseur de processeur (la valeur recommandée de la fiche, ou « Hearth ne sait pas… »). */
    cpuNote?: string | null;
    disabled?: boolean;
  }>(),
  { recommended: null, memoryNote: null, cpuNote: null, disabled: false },
);

const emit = defineEmits<{ "update:modelValue": [value: Resources] }>();

const PROOF_KEYS = {
  measured: "resources.proofMeasured",
  measured_idle: "resources.proofMeasuredIdle",
  publisher: "resources.proofPublisher",
} as const;

const memoryList = computed(() => (props.capacity ? memoryStops(props.capacity.memoryBytes) : []));
const cpuList = computed(() => (props.capacity ? cpuStops(props.capacity.cpus) : []));

function setMemory(memoryBytes: number | null): void {
  emit("update:modelValue", { ...props.modelValue, memoryBytes });
}

function setCpu(cpuMillis: number | null): void {
  emit("update:modelValue", { ...props.modelValue, cpuMillis });
}

const below = computed(
  () =>
    props.recommended !== null &&
    props.modelValue.memoryBytes !== null &&
    props.modelValue.memoryBytes < props.recommended.bytes,
);
const recommendedText = computed(() =>
  props.recommended === null
    ? ""
    : t("resources.recommended", {
        value: formatMemory(props.recommended.bytes),
        proof: t(PROOF_KEYS[props.recommended.proof]),
      }),
);
const proposedText = computed(() =>
  props.capacity
    ? t("resources.proposed", { value: formatMemory(proposedMemoryBytes(props.capacity)) })
    : "",
);
</script>

<template>
  <section class="sliders" :aria-label="t('resources.region')">
    <Callout v-if="capacity === null" tone="error" role="alert">
      {{ t("resources.noCapacity") }}
    </Callout>
    <template v-else>
      <ResourceSlider
        kind="memory"
        :stops="memoryList"
        :model-value="modelValue.memoryBytes"
        :disabled="disabled"
        @update:model-value="setMemory"
      >
        <template #hint>
          <p v-if="recommended" class="sliders__hint">{{ recommendedText }}</p>
          <p v-else-if="memoryNote" class="sliders__hint" data-memory-note>{{ memoryNote }}</p>
          <p v-else class="sliders__hint">{{ proposedText }}</p>
          <Callout v-if="below" tone="warn" inline>{{ t("resources.belowRecommended") }}</Callout>
        </template>
      </ResourceSlider>
      <ResourceSlider
        kind="cpu"
        :stops="cpuList"
        :model-value="modelValue.cpuMillis"
        :disabled="disabled"
        @update:model-value="setCpu"
      >
        <template v-if="cpuNote" #hint>
          <p class="sliders__hint" data-cpu-note>{{ cpuNote }}</p>
        </template>
      </ResourceSlider>
    </template>
  </section>
</template>

<style scoped>
.sliders {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
  min-width: 0;
}

.sliders__hint {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
