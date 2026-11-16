<script setup lang="ts">
import { useId } from "vue";
import { t } from "@/i18n";

// L'encart de résumé de l'étape « Réglages » (HRT-244) : ce que l'on installe et où, ce que dit la fiche, la mémoire en
// une phrase, l'adresse prévue pour les joueurs, et la promesse qu'aucun geste n'a eu lieu. Des phrases déjà composées
// (`fiche/memory.ts`, `fiche/facts.ts`) : l'encart ne calcule rien.
defineProps<{
  /** « Terraria sur Maison » : le jeu et le serveur qui l'accueillera. */
  heading: string;
  /** La phrase de la fiche (ce que le jeu est). */
  summary: string;
  /** La mémoire en une phrase, avec sa preuve. */
  memory: string;
  /** « 192.168.1.20:7777 », ou `null` quand la fiche ne publie aucun port. */
  address: string | null;
}>();

const titleId = useId();
</script>

<template>
  <aside class="aside" :aria-labelledby="titleId" data-install-aside>
    <h2 :id="titleId" class="aside__title">{{ heading }}</h2>
    <p class="aside__line" data-aside-summary>{{ summary }}</p>
    <p class="aside__line" data-aside-memory>{{ memory }}</p>
    <p v-if="address" class="aside__line" data-aside-address>
      {{ t("fiche.aside.address") }}
      <span class="aside__address">{{ address }}</span>
    </p>
    <p class="aside__note" data-aside-note>{{ t("fiche.aside.nothingCreated") }}</p>
  </aside>
</template>

<style scoped>
.aside {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.aside__title {
  margin: 0;
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.aside__line,
.aside__note {
  margin: 0;
  color: var(--tx-secondary);
  overflow-wrap: anywhere;
}

.aside__address {
  color: var(--tx);
  font-family: var(--font-mono);
}

.aside__note {
  padding-top: var(--space-3);
  border-top: var(--border-width) solid var(--bd);
}
</style>
