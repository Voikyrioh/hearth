<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import HIcon from "@/components/atoms/HIcon.vue";
import { formatDateTime } from "@/composables/format";
import { t } from "@/i18n";
import type { Role, SecurityAlert } from "@/link";

// Carte « Ce qui se passe » de la page Sécurité (HRT-39, C36, FIX:01M4DJZB43SA08NE46DGEZK213) : ce que
// promet « Plus d'infos ». Elle dit ce que l'agent sait d'une alerte : si ton identifiant est visé et
// depuis quand, combien d'AUTRES comptes le sont (administrateur seulement, jamais leurs noms : ils
// se lisent dans le journal d'activité), et quoi faire. L'agent ne livre ni adresse ni nombre d'essais :
// la carte ne les invente pas. Elle ne porte PAS de bouton « Activer » : celui de la carte « Mode attaque »,
// juste dessous, est le seul de la page.
const props = defineProps<{
  alert: SecurityAlert;
  role: Role;
  serverId: string;
  modeOn: boolean;
}>();

const others = computed(() => props.alert.others ?? 0);
const since = computed(() => (props.alert.since ? formatDateTime(props.alert.since) : null));
</script>

<template>
  <section class="card" aria-labelledby="alert-card-title" data-alert-card>
    <header class="card__head">
      <HIcon name="alert" class="card__icon" />
      <h2 id="alert-card-title" class="card__title">{{ t("security.cardTitle") }}</h2>
    </header>
    <ul class="card__facts">
      <li v-if="alert.own" data-alert-own>
        {{ since ? t("security.cardOwnSince", { time: since }) : t("security.cardOwn") }}
      </li>
      <li v-if="role === 'admin' && others > 0" data-alert-others>
        {{ t(others === 1 ? "security.cardOthersOne" : "security.cardOthersMany", { n: others }) }}
        <RouterLink class="card__link" :to="{ name: 'audit', params: { id: serverId } }">
          {{ t("security.cardSeeJournal") }}
        </RouterLink>
      </li>
    </ul>
    <p class="card__todo" data-alert-todo>
      {{ t(modeOn ? "security.cardTodoOn" : "security.cardTodoOff") }}
    </p>
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border: var(--border-width) solid var(--warn);
  border-radius: var(--radius-card);
  background: var(--warn-tint);
}

.card__head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.card__icon {
  color: var(--warn);
}

.card__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.card__facts {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-left: var(--space-4);
}

.card__link {
  margin-left: var(--space-2);
  color: var(--ac);
  text-decoration: underline;
}

.card__todo {
  color: var(--tx2);
}
</style>
