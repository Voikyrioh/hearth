<script setup lang="ts">
import { computed, watch } from "vue";
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
  resourcesIssue,
} from "@/resources";

// Mémoire et processeur d'un service qu'on crée (HRT-146). Les bornes viennent de la machine (`capacity`, lue sur
// `GET /engine/capacity`) ; la valeur de départ est donnée par l'appelant (`startingResources`). Il n'existe aucune
// position « sans limite » (BR-RES-037) : une valeur vide est refusée sur place et `valid` reste faux, de même
// quand les bornes ne sont pas lues. Le composant ne réclame rien d'autre : l'appelant (déploiement, installation
// par fiche) garde son bouton d'envoi inactif tant que `valid` est faux.
const props = withDefaults(
  defineProps<{
    /** Les bornes de la machine ; `null` tant que le serveur n'a pas répondu. */
    capacity: EngineCapacity | null;
    modelValue: Resources;
    /** La mémoire recommandée d'une fiche et sa preuve ; `null` pour une application sans fiche. */
    recommended?: { bytes: number; proof: "measured" | "publisher" } | null;
    disabled?: boolean;
  }>(),
  { recommended: null, disabled: false },
);

const emit = defineEmits<{
  "update:modelValue": [value: Resources];
  "update:valid": [valid: boolean];
}>();

const memoryList = computed(() => (props.capacity ? memoryStops(props.capacity.memoryBytes) : []));
const cpuList = computed(() => (props.capacity ? cpuStops(props.capacity.cpus) : []));

const valid = computed(() => resourcesIssue(props.capacity, props.modelValue) === null);
watch(valid, (value) => emit("update:valid", value), { immediate: true });

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
        proof: t(
          props.recommended.proof === "measured"
            ? "resources.proofMeasured"
            : "resources.proofPublisher",
        ),
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
          <template v-else>
            <p class="sliders__hint">{{ t("resources.noFigure") }}</p>
            <p class="sliders__hint">{{ proposedText }}</p>
          </template>
          <Callout v-if="below" tone="warn" inline>{{ t("resources.belowRecommended") }}</Callout>
        </template>
      </ResourceSlider>
      <ResourceSlider
        kind="cpu"
        :stops="cpuList"
        :model-value="modelValue.cpuMillis"
        :disabled="disabled"
        @update:model-value="setCpu"
      />
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
