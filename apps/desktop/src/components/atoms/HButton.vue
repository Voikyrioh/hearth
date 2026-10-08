<script setup lang="ts">
import { computed, useId } from "vue";
import { type NeedsLink, useNeedsLink } from "@/composables/useNeedsLink";
import HSpinner from "./HSpinner.vue";
import HTooltip from "./HTooltip.vue";

// Désactivé = `aria-disabled` (et non `disabled`) : le bouton garde le focus clavier et son
// infobulle, qui explique pourquoi il est inactif. L'état final se calcule ICI :
// `disabled` OU `busy` OU « le serveur manque » (`needsLink`, voir `useNeedsLink`). `hint`
// est l'explication d'un blocage fixe ; l'explication du lien, plus spécifique, la remplace.
// `solid` (destructeur seulement) : plein, réservé à la confirmation finale.
defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "danger" | "ghost";
    size?: "sm" | "md" | "lg";
    disabled?: boolean;
    busy?: boolean;
    solid?: boolean;
    needsLink?: NeedsLink;
    hint?: string;
    /** FIX:01M4E5D6DYHHX9CBBVDYAFEWMZ (C42) : la raison du blocage est AUSSI écrite sous le bouton, en texte permanent (pas seulement au survol). */
    reasonBelow?: boolean;
    tipPlacement?: "start" | "center" | "end";
    type?: "button" | "submit";
  }>(),
  {
    variant: "primary",
    size: "md",
    disabled: false,
    busy: false,
    solid: false,
    needsLink: undefined,
    hint: undefined,
    reasonBelow: false,
    tipPlacement: "center",
    type: "button",
  },
);

const emit = defineEmits<{ click: [event: MouseEvent] }>();

const linkReason = useNeedsLink(() => props.needsLink);
const inert = computed(() => props.disabled || props.busy || linkReason.value !== null);
const tip = computed(() => linkReason.value ?? (props.disabled ? props.hint : undefined));

const reason = computed(() =>
  props.reasonBelow && inert.value && !props.busy ? tip.value : undefined,
);
// La raison écrite sous le bouton n'est dite qu'une fois : pas d'infobulle redondante, et le texte visible décrit le bouton.
const reasonId = useId();

function onClick(event: MouseEvent) {
  if (inert.value) {
    event.preventDefault();
    return;
  }
  emit("click", event);
}
</script>

<template>
  <span :class="reasonBelow ? 'btn-with-reason' : 'btn-bare'">
    <HTooltip :text="reason ? undefined : tip" :placement="tipPlacement">
      <template #default="{ describedby }">
        <button
          v-bind="$attrs"
          :type="type"
          :class="[
            'btn',
            `btn--${variant}`,
            `btn--${size}`,
            { 'btn--busy': busy, 'btn--solid': solid && variant === 'danger' },
          ]"
          :aria-disabled="inert ? 'true' : undefined"
          :aria-busy="busy ? 'true' : undefined"
          :aria-describedby="reason ? reasonId : describedby"
          @click="onClick"
        >
          <HSpinner v-if="busy" />
          <slot />
        </button>
      </template>
    </HTooltip>
    <span v-if="reason" :id="reasonId" class="btn__reason" data-button-reason>{{ reason }}</span>
  </span>
</template>

<style scoped>
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-2);
  min-height: var(--target-size);
  padding: 0 var(--space-4);
  border: var(--border-width) solid transparent;
  border-radius: var(--radius-control);
  font-weight: var(--fw-semibold);
  white-space: nowrap;
  cursor: pointer;
  transition:
    background var(--motion-fast) var(--ease),
    border-color var(--motion-fast) var(--ease);
}

/* Sans raison écrite, l'enveloppe ne change aucune mise en page. */
.btn-bare {
  display: contents;
}

.btn-with-reason {
  display: inline-flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.btn__reason {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.btn--sm {
  min-height: var(--control-sm);
  padding: 0 var(--space-3);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
}

.btn--lg {
  min-height: var(--control-lg);
  padding: 0 var(--space-5);
  font-size: var(--fs-lead);
}

.btn--primary {
  background: var(--ac);
  color: var(--on-ac);
}

.btn--primary:hover:not([aria-disabled="true"]) {
  background: var(--ac-hover);
}

.btn--secondary {
  border-color: var(--bd);
  background: transparent;
  color: var(--tx);
}

.btn--secondary:hover:not([aria-disabled="true"]) {
  background: var(--card-2);
}

.btn--danger {
  border-color: var(--crit);
  background: transparent;
  color: var(--crit);
}

.btn--danger:hover:not([aria-disabled="true"]) {
  background: var(--crit-hover);
}

.btn--danger.btn--solid {
  background: var(--crit);
  color: var(--on-crit);
}

.btn--ghost {
  min-width: var(--target-size);
  padding: 0;
  background: transparent;
  color: var(--tx2);
}

.btn--ghost:hover:not([aria-disabled="true"]) {
  background: var(--card-2);
  color: var(--tx);
}

.btn--busy {
  cursor: progress;
}
</style>
