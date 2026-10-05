<script setup lang="ts">
import { computed } from "vue";
import { formatSeen } from "@/composables/format";
import { useNow } from "@/composables/useNow";

// « Vu il y a 12 s » : âge du dernier contact, mis à jour en direct (BR-RESIL-007).
const props = defineProps<{ lastContactAt: number | null }>();

const now = useNow();
const text = computed(() => formatSeen(props.lastContactAt, now.value));
</script>

<template>
  <span class="stamp">{{ text }}</span>
</template>

<style scoped>
.stamp {
  color: var(--tx3);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
</style>
