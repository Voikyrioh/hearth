<script setup lang="ts">
import { computed } from "vue";
import { type NeedsLink, useNeedsLink } from "@/composables/useNeedsLink";
import HTooltip from "./HTooltip.vue";

// Même convention que HButton : désactivé = `aria-disabled` (focus conservé), l'état final
// se calcule ici (`disabled` OU `busy` OU `needsLink`) et l'explication s'affiche par infobulle.
// `busy` bloque un second clic pendant qu'une commande tourne.
defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    modelValue: boolean;
    disabled?: boolean;
    busy?: boolean;
    needsLink?: NeedsLink;
    hint?: string;
  }>(),
  { disabled: false, busy: false, needsLink: undefined, hint: undefined },
);

const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();

const linkReason = useNeedsLink(() => props.needsLink);
const inert = computed(() => props.disabled || props.busy || linkReason.value !== null);
const tip = computed(() => linkReason.value ?? (props.disabled ? props.hint : undefined));

function onClick() {
  if (inert.value) return;
  emit("update:modelValue", !props.modelValue);
}
</script>

<template>
  <HTooltip :text="tip" placement="end">
    <template #default="{ describedby }">
      <button
        v-bind="$attrs"
        type="button"
        role="switch"
        :class="['toggle', { 'toggle--on': modelValue, 'toggle--busy': busy }]"
        :aria-checked="modelValue"
        :aria-disabled="inert ? 'true' : undefined"
        :aria-busy="busy ? 'true' : undefined"
        :aria-describedby="describedby"
        @click="onClick"
      >
        <span class="toggle__knob" />
      </button>
    </template>
  </HTooltip>
</template>

<style scoped>
.toggle {
  position: relative;
  flex: none;
  width: var(--toggle-width);
  height: var(--toggle-height);
  padding: 0;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  background: var(--bg);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease);
}

.toggle--on {
  border-color: var(--ac);
  background: var(--ac);
}

.toggle--busy {
  cursor: progress;
}

.toggle__knob {
  position: absolute;
  top: var(--toggle-inset);
  left: var(--toggle-inset);
  width: var(--toggle-knob);
  height: var(--toggle-knob);
  border-radius: var(--radius-pill);
  background: var(--tx);
  transition: transform var(--motion-fast) var(--ease);
}

.toggle--on .toggle__knob {
  background: var(--on-ac);
  transform: translateX(var(--toggle-travel));
}
</style>
