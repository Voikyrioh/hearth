<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon, { type IconName } from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";
import { type ToastKind, useToastsStore } from "@/stores/toasts";

// Notifications empilées en bas à droite : 3 visibles au plus, compteur sur les répétitions,
// jamais bloquantes (aucun focus volé), fermables, annoncées aux lecteurs d'écran.
const toasts = useToastsStore();

const ICONS: Record<ToastKind, IconName> = {
  info: "info",
  success: "check",
  warn: "alert",
  error: "alert",
};
</script>

<template>
  <section class="stack" :aria-label="t('toast.region')" aria-live="polite" aria-relevant="additions">
    <div
      v-for="toast in toasts.visible"
      :key="toast.id"
      :class="['toast', `toast--${toast.kind}`]"
      :data-kind="toast.kind"
    >
      <HIcon class="toast__icon" :name="ICONS[toast.kind]" />
      <p class="toast__message">{{ toast.message }}</p>
      <span v-if="toast.count > 1" class="toast__count">{{
        t("toast.repeated", { n: toast.count })
      }}</span>
      <HButton
        variant="ghost"
        size="sm"
        :aria-label="t('toast.close')"
        @click="toasts.dismiss(toast.id)"
      >
        <HIcon name="close" size="sm" />
      </HButton>
    </div>
  </section>
</template>

<style scoped>
.stack {
  position: fixed;
  right: var(--page-pad);
  bottom: var(--page-pad);
  z-index: var(--z-toast);
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: var(--toast-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-3) var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  pointer-events: auto;
}

.toast--success .toast__icon {
  color: var(--ok);
}

.toast--warn .toast__icon {
  color: var(--warn);
}

.toast--error {
  border-color: var(--crit);
}

.toast--error .toast__icon {
  color: var(--crit);
}

.toast__message {
  flex: 1;
  min-width: 0;
}

.toast__count {
  padding: 2px var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--bd);
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: nowrap;
}
</style>
