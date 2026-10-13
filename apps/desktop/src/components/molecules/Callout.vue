<script setup lang="ts">
import { computed } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";

// Encart d'information, d'avertissement, d'erreur ou de réussite : un pictogramme, un texte, des actions en
// option. Le sens ne repose jamais sur la couleur seule : le pictogramme et le texte le portent.
// `role="status"` par défaut (annonce polie), `alert` pour une erreur bloquante. `inline` : encart discret
// posé sous un champ ou un bloc.
const props = withDefaults(
  defineProps<{
    tone?: "info" | "warn" | "error" | "ok";
    role?: "status" | "alert";
    inline?: boolean;
  }>(),
  { tone: "info", role: "status", inline: false },
);

const icon = computed(() => {
  switch (props.tone) {
    case "warn":
    case "error":
      return "alert";
    case "ok":
      return "check";
    default:
      return "info";
  }
});
</script>

<template>
  <div :class="['callout', `callout--${tone}`, { 'callout--inline': inline }]" :role="role">
    <HIcon class="callout__icon" :name="icon" size="sm" />
    <div class="callout__body"><slot /></div>
    <div v-if="$slots.actions" class="callout__actions"><slot name="actions" /></div>
  </div>
</template>

<style scoped>
.callout {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
}

.callout--inline {
  padding: var(--space-2) var(--space-3);
  font-size: var(--fs-small);
}

.callout--info {
  background: var(--cool-tint);
}

.callout--warn {
  background: var(--warn-tint);
}

.callout--error {
  background: var(--crit-tint);
}

.callout--ok {
  background: var(--ok-tint);
}

.callout__icon {
  margin-top: var(--space-half);
}

.callout--info .callout__icon {
  color: var(--cool);
}

.callout--warn .callout__icon {
  color: var(--warn);
}

.callout--error .callout__icon {
  color: var(--crit);
}

.callout--ok .callout__icon {
  color: var(--ok);
}

.callout__body {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}

.callout__actions {
  display: flex;
  flex: none;
  align-items: center;
  gap: var(--space-2);
}
</style>
