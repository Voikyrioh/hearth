<script setup lang="ts">
import { computed, onMounted, ref, useId, watch } from "vue";
import HInput from "@/components/atoms/HInput.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import { t } from "@/i18n";
import {
  CPU_STEP_MILLIS,
  cpuFieldOf,
  cpuFromField,
  formatCpu,
  formatMemory,
  type MemoryUnit,
  memoryFieldOf,
  memoryFromField,
  nearestStopIndex,
} from "@/resources";

// Une ressource à régler : un curseur discret, qui parcourt la liste des valeurs permises, et à côté une saisie
// pour une valeur exacte (HRT-146, BR-RES-004). Il n'y a ni case, ni position, ni saisie qui retire la limite
// (BR-RES-037) : une saisie vide ou refusée met la valeur à `null` et le dit sur place. Le slot `hint` porte ce que
// l'appelant dit sous le curseur (mémoire recommandée, preuve, avertissements).
const props = withDefaults(
  defineProps<{
    kind: "memory" | "cpu";
    /** Valeurs permises, croissantes, en octets (mémoire) ou en millicœurs (processeur). */
    stops: readonly number[];
    /** Valeur courante ; `null` quand la saisie est vide ou refusée. */
    modelValue: number | null;
    disabled?: boolean;
  }>(),
  { disabled: false },
);

const emit = defineEmits<{ "update:modelValue": [value: number | null] }>();

const id = useId();
const min = computed(() => props.stops[0] ?? 0);
const max = computed(() => props.stops.at(-1) ?? 0);

const text = ref("");
const unit = ref<MemoryUnit>("mo");
const touched = ref(false);
/** Position du curseur : celle de la dernière valeur lisible, pour qu'une saisie en cours ne le fasse pas sauter. */
const index = ref(0);

function parsed(): number | null {
  const value =
    props.kind === "memory" ? memoryFromField(text.value, unit.value) : cpuFromField(text.value);
  if (value === null || value < min.value || value > max.value) return null;
  // Le processeur se règle par pas de 0,25 cœur (BR-RES-004).
  return props.kind === "cpu" && value % CPU_STEP_MILLIS !== 0 ? null : value;
}

function writeField(value: number): void {
  if (props.kind === "memory") {
    const field = memoryFieldOf(value);
    text.value = field.text;
    unit.value = field.unit;
  } else {
    text.value = cpuFieldOf(value);
  }
}

// La valeur vient d'ailleurs (curseur, appelant) : la saisie la recopie, sauf si elle la dit déjà. Les crans
// peuvent changer sous une même valeur (bornes relues) : le curseur se replace alors sur la nouvelle liste.
watch(
  [() => props.modelValue, () => props.stops],
  ([value]) => {
    if (value === null) return;
    index.value = nearestStopIndex(props.stops, value);
    if (parsed() !== value) writeField(value);
  },
  { immediate: true },
);

function onText(value: string): void {
  text.value = value;
  typed();
}

function onUnit(value: MemoryUnit): void {
  unit.value = value;
  typed();
}

function typed(): void {
  touched.value = true;
  const value = parsed();
  if (value !== null) index.value = nearestStopIndex(props.stops, value);
  emit("update:modelValue", value);
}

function moved(event: Event): void {
  const next = props.stops[Number((event.target as HTMLInputElement).value)];
  if (next === undefined) return;
  touched.value = false;
  index.value = nearestStopIndex(props.stops, next);
  writeField(next);
  emit("update:modelValue", next);
}

const error = computed(() => {
  if (!touched.value || parsed() !== null) return undefined;
  if (text.value.trim() === "") {
    return t(props.kind === "memory" ? "resources.memoryEmpty" : "resources.cpuEmpty");
  }
  return props.kind === "memory"
    ? t("resources.memoryRange", { min: formatMemory(min.value), max: formatMemory(max.value) })
    : t("resources.cpuRange", { n: max.value / 1000 });
});

const format = (value: number) =>
  props.kind === "memory" ? formatMemory(value) : formatCpu(value);
const valueText = computed(() => format(props.stops[index.value] ?? min.value));
const sliderLabel = computed(() =>
  t(props.kind === "memory" ? "resources.memorySlider" : "resources.cpuSlider"),
);
const fieldLabel = computed(() =>
  t(props.kind === "memory" ? "resources.memoryField" : "resources.cpuField"),
);
const title = computed(() => t(props.kind === "memory" ? "resources.memory" : "resources.cpu"));
const unitOptions = [
  { value: "mo" as const, label: t("resources.unitMoOption") },
  { value: "go" as const, label: t("resources.unitGoOption") },
];

// Le remplissage de la piste suit la poignée : seule la variable CSS `--fill` est écrite dans le style de
// l'élément, depuis le script ; le gabarit n'a pas d'attribut `style=` et les couleurs restent dans le bloc <style>.
const slider = ref<HTMLInputElement | null>(null);
function paint(): void {
  const last = Math.max(props.stops.length - 1, 1);
  slider.value?.style.setProperty("--fill", `${(index.value / last) * 100}%`);
}
onMounted(paint);
watch([index, () => props.stops], paint);
</script>

<template>
  <div class="resource" :data-kind="kind" role="group" :aria-labelledby="`${id}-title`">
    <h4 :id="`${id}-title`" class="resource__title">{{ title }}</h4>
    <div class="resource__row">
      <div class="resource__track">
        <input
          ref="slider"
          class="resource__slider"
          type="range"
          min="0"
          :max="Math.max(stops.length - 1, 0)"
          step="1"
          :value="index"
          :disabled="disabled"
          :aria-label="sliderLabel"
          :aria-valuetext="valueText"
          @input="moved"
        />
        <div class="resource__ends" aria-hidden="true">
          <span>{{ format(min) }}</span>
          <span>{{ format(max) }}</span>
        </div>
      </div>
      <div class="resource__field">
        <div class="resource__input">
          <HInput
            :model-value="text"
            :label="fieldLabel"
            mono
            :disabled="disabled"
            :error="error"
            @update:model-value="onText"
          />
        </div>
        <div v-if="kind === 'memory'" class="resource__unit">
          <HSelect
            :model-value="unit"
            :label="t('resources.memoryUnit')"
            :options="unitOptions"
            :disabled="disabled"
            hide-label
            @update:model-value="onUnit"
          />
        </div>
        <span v-else class="resource__suffix">{{ t("resources.unitCoresSuffix") }}</span>
      </div>
    </div>
    <slot name="hint" />
  </div>
</template>

<style scoped>
.resource {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.resource__title {
  color: var(--tx);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.resource__row {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-4) var(--space-5);
}

.resource__track {
  flex: 1 1 0;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding-bottom: var(--space-2);
}

.resource__ends {
  display: flex;
  justify-content: space-between;
  color: var(--tx3);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.resource__field {
  display: flex;
  align-items: flex-end;
  gap: var(--space-2);
}

.resource__input {
  width: var(--resource-field-width);
}

.resource__unit {
  width: var(--resource-unit-width);
}

.resource__suffix {
  padding-bottom: var(--field-pad-y);
  color: var(--tx-secondary);
}

.resource__slider {
  --fill: 0%;
  appearance: none;
  width: 100%;
  height: var(--slider-thumb);
  margin: 0;
  background: transparent;
  cursor: pointer;
}

.resource__slider:disabled {
  cursor: default;
  opacity: var(--opacity-disabled);
}

.resource__slider:focus-visible {
  outline: none;
}

.resource__slider::-webkit-slider-runnable-track {
  height: var(--slider-track);
  border-radius: var(--radius-pill);
  background: linear-gradient(to right, var(--ac) var(--fill), var(--bd) var(--fill));
}

.resource__slider::-webkit-slider-thumb {
  appearance: none;
  width: var(--slider-thumb);
  height: var(--slider-thumb);
  margin-top: calc((var(--slider-track) - var(--slider-thumb)) / 2);
  border: var(--border-width) solid var(--tx);
  border-radius: 50%;
  background: var(--ac);
}

.resource__slider:focus-visible::-webkit-slider-thumb {
  box-shadow: 0 0 0 var(--halo-width) var(--focus-halo);
}
</style>
