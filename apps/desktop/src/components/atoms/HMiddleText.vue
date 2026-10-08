<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import HTooltip from "./HTooltip.vue";

// Texte long tronqué AU MILIEU : le début et la fin restent visibles (un chemin se reconnaît à ses deux bouts), le
// milieu devient « … ». La troncature est VISUELLE seulement : le texte COMPLET est dans le document (`sr-only`), les deux
// moitiés affichées sont masquées aux lecteurs d'écran, donc le lecteur lit le chemin entier et une sélection le copie
// entier. Quand le texte est tronqué, il prend le focus clavier et le texte complet apparaît au survol et au focus
// (infobulle). Un texte qui tient reste du texte simple, sans arrêt de tabulation (HRT-47, S1b).
const props = withDefaults(defineProps<{ text: string; tail?: number }>(), { tail: 12 });

const head = computed(() =>
  props.text.length > props.tail ? props.text.slice(0, -props.tail) : "",
);
const end = computed(() =>
  props.text.length > props.tail ? props.text.slice(-props.tail) : props.text,
);

const start = ref<HTMLElement | null>(null);
const truncated = ref(false);
let observer: ResizeObserver | undefined;
function measure() {
  const element = start.value;
  truncated.value = element !== null && element.scrollWidth > element.clientWidth + 1;
}
onMounted(() => {
  measure();
  if (typeof ResizeObserver !== "undefined") observer = new ResizeObserver(measure);
  if (start.value) observer?.observe(start.value);
});
// Le texte change (un disque monté après coup, un nom qui s'allonge) : la moitié « début » peut apparaître, disparaître ou
// être un autre élément ; on la suit et on remesure.
watch(start, (element, previous) => {
  if (previous) observer?.unobserve(previous);
  if (element) observer?.observe(element);
});
watch(
  () => props.text,
  async () => {
    await nextTick();
    measure();
  },
);
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <HTooltip :text="truncated ? text : undefined" placement="start">
    <span class="mid" data-middle :tabindex="truncated ? 0 : undefined">
      <span class="sr-only">{{ text }}</span>
      <span v-if="head" ref="start" class="mid__start" aria-hidden="true">{{ head }}</span>
      <span class="mid__end" aria-hidden="true">{{ end }}</span>
    </span>
  </HTooltip>
</template>

<style scoped>
.mid {
  display: inline-flex;
  min-width: 0;
  max-width: 100%;
}

.mid__start {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.mid__end {
  flex: none;
  white-space: nowrap;
}
</style>
