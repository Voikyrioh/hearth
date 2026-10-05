<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { formatClock } from "@/composables/format";
import { t } from "@/i18n";

// Bandeau « Hors ligne » (BR-RESIL-004) : heure du dernier contact, nouvelle tentative
// automatique, bouton pour réessayer tout de suite. Jamais modal.
const props = defineProps<{ lastContactAt: number | null }>();

defineEmits<{ retry: [] }>();

const text = computed(() =>
  props.lastContactAt === null
    ? t("link.offlineBannerNoContact")
    : t("link.offlineBanner", { time: formatClock(props.lastContactAt) }),
);
</script>

<template>
  <div class="banner" role="status">
    <HIcon class="banner__icon" name="alert" />
    <p class="banner__text">{{ text }}</p>
    <HButton variant="secondary" size="sm" @click="$emit('retry')">
      <HIcon name="refresh" size="sm" />
      {{ t("link.retryNow") }}
    </HButton>
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--crit);
  border-radius: var(--radius-control);
  background: var(--crit-tint);
}

.banner__icon {
  color: var(--crit);
}

.banner__text {
  flex: 1;
  min-width: 0;
}
</style>
