<script setup lang="ts">
withDefaults(defineProps<{ title: string; text: string; heading?: "h1" | "h2" }>(), {
  heading: "h1",
});
</script>

<template>
  <section class="empty">
    <div v-if="$slots.illustration" class="empty__illustration">
      <slot name="illustration" />
    </div>
    <component :is="heading" class="empty__title">{{ title }}</component>
    <p class="empty__text">{{ text }}</p>
    <div v-if="$slots.action" class="empty__action">
      <slot name="action" />
    </div>
  </section>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: 100%;
  max-width: var(--content-max);
  text-align: center;
}

.empty__illustration {
  margin-bottom: var(--space-5);
}

.empty__title {
  font-size: var(--fs-h1);
  font-weight: var(--fw-semibold);
}

.empty__text {
  margin-top: var(--space-3);
  color: var(--tx2);
  font-size: var(--fs-lead);
}

.empty__action {
  margin-top: var(--space-5);
}
</style>
