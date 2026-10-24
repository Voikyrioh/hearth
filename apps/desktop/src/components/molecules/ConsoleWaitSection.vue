<script setup lang="ts">
import { useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";

// « Ce qui attend la console » (HRT-109, BR-FICHE-051) : sans console, tout ce qui est faisable est livré, et chaque
// fonction que le jeu offre par sa console et que Hearth n'offre pas encore est NOMMÉE, avec le texte montré à sa place.
// Le bouton n'est pas caché : il est présent, désactivé, avec « Cette fonction attend la console de Hearth, qui n'est
// pas encore livrée. » suivi du contournement. Ces trois fonctions valent pour tout jeu (appliquer sans redémarrer, voir
// qui est connecté, écrire aux joueurs) ; celles qui ne sont propres qu'à un jeu viendront de la fiche. Repliée.
const titleId = useId();
const functions = [
  { id: "apply", name: "gameSettings.consoleApply", way: "gameSettings.consoleApplyWay" },
  { id: "who", name: "gameSettings.consoleWho", way: "gameSettings.consoleWhoWay" },
  { id: "tell", name: "gameSettings.consoleTell", way: "gameSettings.consoleTellWay" },
] as const;
</script>

<template>
  <details class="console" data-console-wait>
    <summary class="console__summary">
      <h2 :id="titleId" class="console__title">{{ t("gameSettings.consoleTitle") }}</h2>
    </summary>
    <ul class="console__list" :aria-labelledby="titleId">
      <li v-for="item in functions" :key="item.id" class="console__item" :data-console-function="item.id">
        <div class="console__text">
          <p class="console__name">{{ t(item.name) }}</p>
          <p class="console__way">{{ t("gameSettings.consoleLead") }} {{ t(item.way) }}</p>
        </div>
        <HButton variant="secondary" size="sm" disabled>{{ t("gameSettings.consoleSoon") }}</HButton>
      </li>
    </ul>
  </details>
</template>

<style scoped>
.console {
  min-width: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.console__summary {
  padding: var(--space-4) var(--space-5);
  cursor: pointer;
}

.console__summary:focus-visible {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.console__title {
  display: inline;
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.console__list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0 var(--space-5) var(--space-4);
  list-style: none;
}

.console__item {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) 0;
  border-bottom: var(--border-width) solid var(--bd);
}

.console__item:last-child {
  border-bottom: 0;
}

.console__text {
  flex: 1 1 0;
  min-width: 0;
}

.console__name {
  margin: 0;
  font-weight: var(--fw-medium);
}

.console__way {
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
