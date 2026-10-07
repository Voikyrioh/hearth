<script setup lang="ts">
import { t } from "@/i18n";

// Un seul dessin pour toutes les tailles : flamme pleine à deux formes, pointe ronde
// (`src-tauri/icons/source/logo.svg`, ADR-0027). « size » ne change que la taille.
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
    <path
      d="M146 190V260A110 110 0 0 0 366 260V190"
      fill="none"
      stroke="currentColor"
      stroke-width="46"
      stroke-linecap="round"
    />
    <path
      fill="currentColor"
      d="M196 258C196 215 221 170 250.8 129.6A14 14 0 0 1 276 138C276 172 316 205 316 258A60 60 0 0 1 196 258Z"
    />
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
