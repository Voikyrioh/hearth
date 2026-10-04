<script setup lang="ts">
defineProps<{ title: string; text: string }>();
</script>

<template>
  <section class="empty">
    <div v-if="$slots.illustration" class="empty__illustration">
      <slot name="illustration" />
    </div>
    <h1 class="empty__title">{{ title }}</h1>
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
  font-weight: 600;
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
