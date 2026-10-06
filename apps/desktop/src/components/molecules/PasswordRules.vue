<script setup lang="ts">
import { PASSWORD_CRITERIA, ruleText } from "@/accounts/messages";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";
import type { PasswordRule } from "@/link";

// Critères du mot de passe en direct : une coche (respecté) ou une croix (non respecté) ET un texte
// pour les lecteurs d'écran, jamais la couleur seule. `unmet` vient de l'agent (règle de
// `hearth-proto`, évaluée par la coquille) : ce composant n'en connaît aucune. Avant la première
// saisie (`touched` faux) les critères sont montrés en retrait, sans rouge.
// `required` (rien n'est saisi) veut dire « rien n'est encore évalué » : aucun critère n'est alors
// « Respecté », tout est neutre. C'est ICI, à un seul endroit, que l'état vide est traité.
const props = defineProps<{ unmet: readonly PasswordRule[]; touched: boolean }>();

type CriterionState = "met" | "unmet" | "idle" | "pending";

function stateOf(rule: PasswordRule): CriterionState {
  if (props.unmet.includes("required")) return "pending";
  if (!props.unmet.includes(rule)) return "met";
  return props.touched ? "unmet" : "idle";
}
</script>

<template>
  <ul class="rules" :aria-label="t('accounts.passwordRulesLabel')">
    <li
      v-for="rule in PASSWORD_CRITERIA"
      :key="rule"
      :class="['rules__item', `rules__item--${stateOf(rule)}`]"
      :data-rule="rule"
      :data-met="stateOf(rule) === 'met' ? 'true' : stateOf(rule) === 'pending' ? 'pending' : 'false'"
    >
      <HIcon :name="stateOf(rule) === 'met' ? 'check' : 'close'" size="sm" />
      <span>{{ ruleText(rule) }}</span>
      <span class="sr-only">{{
        stateOf(rule) === "met"
          ? t("accounts.met")
          : stateOf(rule) === "pending"
            ? t("accounts.pending")
            : t("accounts.unmet")
      }}</span>
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

.rules__item--idle,
.rules__item--pending {
  color: var(--tx3);
}
</style>
