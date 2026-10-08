<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import HTooltip from "./HTooltip.vue";

// Texte long tronqué AU MILIEU : le début et la fin restent visibles (un chemin se reconnaît à ses deux bouts), le
// milieu devient « … ». Quand le texte est tronqué, il prend le focus clavier et le texte complet apparaît au survol et
// au focus (infobulle) et se lit en entier (`aria-label`). Un texte qui tient reste du texte simple, sans arrêt de
// tabulation (HRT-47, S1b).
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
  if (typeof ResizeObserver === "undefined" || !start.value) return;
  observer = new ResizeObserver(measure);
  observer.observe(start.value);
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <HTooltip :text="truncated ? text : undefined" placement="start">
    <template #default="{ describedby }">
      <span
        class="mid"
        data-middle
        :role="truncated ? 'text' : undefined"
        :tabindex="truncated ? 0 : undefined"
        :aria-label="truncated ? text : undefined"
        :aria-describedby="describedby"
      >
        <span v-if="head" ref="start" class="mid__start" :aria-hidden="truncated ? 'true' : undefined">{{ head }}</span>
        <span class="mid__end" :aria-hidden="truncated ? 'true' : undefined">{{ end }}</span>
      </span>
    </template>
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
