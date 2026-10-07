<script setup lang="ts">
import { ILLUSTRATIONS, type IllustrationName } from "@/assets/illustrations";

// « illustration » : un dessin de la collection (décoratif, texte alternatif vide, le titre dit déjà
// tout). Le slot du même nom prime, pour un contenu qui n'est pas une illustration.
withDefaults(
  defineProps<{
    title: string;
    text: string;
    heading?: "h1" | "h2";
    illustration?: IllustrationName;
    size?: "md" | "lg";
  }>(),
  { heading: "h1", illustration: undefined, size: "md" },
);
</script>

<template>
  <section class="empty">
    <div v-if="$slots.illustration || illustration" class="empty__illustration">
      <slot name="illustration">
        <img
          v-if="illustration"
          :class="['empty__img', `empty__img--${size}`]"
          :src="ILLUSTRATIONS[illustration]"
          alt=""
        />
      </slot>
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

.empty__img {
  display: block;
}

.empty__img--md {
  width: var(--illustration-md);
}

.empty__img--lg {
  width: var(--illustration-lg);
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
