<script setup lang="ts">
import { useId } from "vue";

defineProps<{ label: string; help?: string }>();

// Le contrôle (slot) se nomme par `labelId` : `aria-labelledby`.
const labelId = useId();
</script>

<template>
  <div class="row">
    <div class="row__text">
      <span :id="labelId" class="row__label">{{ label }}</span>
      <p v-if="help" class="row__help">{{ help }}</p>
    </div>
    <slot :label-id="labelId" />
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-5);
  padding: var(--space-3) 0;
}

.row__label {
  font-weight: 500;
}

.row__help {
  margin-top: var(--space-1);
  color: var(--tx2);
}
</style>
