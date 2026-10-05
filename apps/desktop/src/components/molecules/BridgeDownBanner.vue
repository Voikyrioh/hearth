<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// La liste des serveurs n'a pas pu être lue (le pont de liaison est en panne ou ne répond pas) :
// on le dit à l'écran, avec de quoi réessayer. Jamais bloquant : la coquille reste utilisable.
defineProps<{ busy?: boolean }>();

defineEmits<{ retry: [] }>();
</script>

<template>
  <div class="down" role="alert" data-bridge-down>
    <HIcon class="down__icon" name="alert" />
    <p class="down__text">{{ t("bridge.loadFailed") }}</p>
    <HButton variant="secondary" size="sm" :busy="busy" @click="$emit('retry')">
      <HIcon name="refresh" size="sm" />
      {{ t("common.retry") }}
    </HButton>
  </div>
</template>

<style scoped>
.down {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-bottom: var(--border-width) solid var(--warn);
  background: var(--card);
}

.down__icon {
  color: var(--warn);
}

.down__text {
  flex: 1;
  min-width: 0;
}
</style>
