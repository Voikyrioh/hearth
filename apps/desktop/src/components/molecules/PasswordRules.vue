<script setup lang="ts">
import { PASSWORD_CRITERIA, ruleText } from "@/accounts/messages";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";
import type { PasswordRule } from "@/link";

// Critères du mot de passe en direct : une coche (respecté) ou une croix (non respecté) ET un texte
// pour les lecteurs d'écran, jamais la couleur seule. `unmet` vient de l'agent (règle de
// `hearth-proto`, évaluée par la coquille) : ce composant n'en connaît aucune. Avant la première
// saisie (`touched` faux) les critères sont montrés en retrait, sans rouge.
defineProps<{ unmet: readonly PasswordRule[]; touched: boolean }>();
</script>

<template>
  <ul class="rules" :aria-label="t('accounts.passwordRulesLabel')">
    <li
      v-for="rule in PASSWORD_CRITERIA"
      :key="rule"
      :class="[
        'rules__item',
        unmet.includes(rule) ? (touched ? 'rules__item--unmet' : 'rules__item--idle') : 'rules__item--met',
      ]"
      :data-rule="rule"
      :data-met="unmet.includes(rule) ? 'false' : 'true'"
    >
      <HIcon :name="unmet.includes(rule) ? 'close' : 'check'" size="sm" />
      <span>{{ ruleText(rule) }}</span>
      <span class="sr-only">{{ unmet.includes(rule) ? t("accounts.unmet") : t("accounts.met") }}</span>
    </li>
  </ul>
</template>

<style scoped>
.rules {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: var(--fs-small);
}

.rules__item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.rules__item--met {
  color: var(--ok);
}

.rules__item--unmet {
  color: var(--crit);
}

.rules__item--idle {
  color: var(--tx3);
}
</style>
