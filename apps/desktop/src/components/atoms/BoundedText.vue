<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

// Un texte venu du serveur dont la HAUTEUR est bornée (HRT-63, revue de la PR 93) : un texte de 64 Ko ne doit
// jamais pousser le reste de la page à des milliers de pixels. Au-delà de la borne il défile DANS sa zone, qui
// prend alors le focus clavier (flèches, Page suivante) et un nom lu par les lecteurs d'écran ; en dessous, c'est du texte
// simple, sans arrêt de tabulation. Le texte est interpolé, jamais du HTML ; l'appelant le rend inoffensif
// (`safeText`). La borne est celle du jeton `--bounded-max-height`, que le parent peut régler.
const props = defineProps<{ text: string; label?: string }>();

const element = ref<HTMLElement | null>(null);
const scrolls = ref(false);
let observer: ResizeObserver | undefined;

function measure() {
  const found = element.value;
  scrolls.value = found !== null && found.scrollHeight > found.clientHeight + 1;
}
onMounted(() => {
  measure();
  if (typeof ResizeObserver !== "undefined") observer = new ResizeObserver(measure);
  if (element.value) observer?.observe(element.value);
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
  <span
    ref="element"
    class="bounded"
    :tabindex="scrolls ? 0 : undefined"
    :role="scrolls && label ? 'group' : undefined"
    :aria-label="scrolls && label ? label : undefined"
    data-bounded
    :data-scrolls="scrolls ? '' : undefined"
    >{{ text }}</span
  >
</template>

<style scoped>
.bounded {
  display: block;
  max-height: var(--bounded-max-height, var(--command-max-height));
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
</style>
