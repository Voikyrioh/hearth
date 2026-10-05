<script setup lang="ts">
import { t } from "@/i18n";

// « sm » reprend le dessin simplifié pour les petites tailles (hearth-logo-small.svg).
withDefaults(defineProps<{ size?: "sm" | "md" | "lg"; decorative?: boolean }>(), {
  size: "md",
  decorative: false,
});
</script>

<template>
  <svg
    :class="['logo', `logo--${size}`]"
    viewBox="116 108 280 288"
    :role="decorative ? undefined : 'img'"
    :aria-label="decorative ? undefined : t('app.name')"
    :aria-hidden="decorative ? 'true' : undefined"
  >
    <template v-if="size === 'sm'">
      <path
        d="M146 190V260A110 110 0 0 0 366 260V190"
        fill="none"
        stroke="currentColor"
        stroke-width="46"
        stroke-linecap="round"
      />
      <path
        fill="currentColor"
        d="M256 124C262 162 316 204 316 258A60 60 0 0 1 196 258C196 212 236 162 256 124Z"
      />
    </template>
    <template v-else>
      <path
        d="M152 190V260A104 104 0 0 0 360 260V190"
        fill="none"
        stroke="currentColor"
        stroke-width="39"
        stroke-linecap="round"
      />
      <path
        fill="currentColor"
        fill-rule="evenodd"
        d="M256 130C262 165 318 205 318 262A62 62 0 0 1 194 262C194 215 236 165 256 130ZM256 232C258 250 284 270 284 298A28 28 0 0 1 228 298C228 272 248 252 256 232Z"
      />
    </template>
  </svg>
</template>

<style scoped>
.logo {
  display: block;
  color: var(--ac);
  flex: none;
}

.logo--sm {
  width: var(--logo-sm);
}

.logo--md {
  width: var(--logo-md);
}

.logo--lg {
  width: var(--logo-lg);
}
</style>
