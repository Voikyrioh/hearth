<script setup lang="ts">
import { computed } from "vue";

// Une phrase dont certaines valeurs (un compte, une version, une distribution) sont en chasse fixe :
// `text` est le modèle de l'interface avec ses `{jetons}` (texte brut, jamais du HTML), `values` donne
// ce qui remplace chaque jeton, rendu en `<code>`. Un jeton sans valeur reste tel quel.
const props = defineProps<{ text: string; values: Record<string, string> }>();

const parts = computed(() =>
  props.text
    .split(/(\{\w+\})/)
    .filter((part) => part !== "")
    .map((part) => {
      const name = /^\{(\w+)\}$/.exec(part)?.[1];
      const value = name === undefined ? undefined : props.values[name];
      return value === undefined ? { text: part, mono: false } : { text: value, mono: true };
    }),
);
</script>

<template>
  <span><template v-for="(part, index) in parts" :key="index"><code v-if="part.mono" class="mono">{{ part.text }}</code><template v-else>{{ part.text }}</template></template></span>
</template>

<style scoped>
.mono {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}
</style>
