<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";
import { t } from "@/i18n";
import type { FicheCard } from "@/link";

// Une fiche dans la liste du Catalogue (HRT-102, BR-FICHE-003) : le nom du jeu, une phrase, « Livrée avec Hearth
// {version} » et « Voir la fiche ». Rien d'autre : pas d'illustration (aucun visuel sans droit d'usage établi), pas
// de bouton « Installer » (un seul chemin, depuis la fiche), pas de mention de compte (BR-FICHE-004). Un jeu sans
// serveur pour ce système est atténué et le dit. Les textes viennent de la fiche : ils sont montrés tels quels, sans
// jamais être interprétés. Toute la carte ouvre la fiche (revue UX 2026-10-24, constat 21) ; « Voir la fiche » reste le
// point d'entrée du clavier.
const props = defineProps<{
  card: FicheCard;
  /** La version de l'agent qui livre la fiche ; `null` tant qu'elle n'est pas connue. */
  agentVersion: string | null;
}>();

const emit = defineEmits<{ open: [] }>();

const name = computed(() => safeText(props.card.name));
const delivered = computed(() =>
  props.agentVersion === null
    ? t("catalogue.deliveredWithBare")
    : t("catalogue.deliveredWith", { version: safeText(props.agentVersion) }),
);
</script>

<template>
  <article
    :class="['fcard', { 'fcard--dim': !card.installable }]"
    data-fiche-card
    :data-fiche="card.id"
    @click="emit('open')"
  >
    <div class="fcard__text">
      <h2 class="fcard__name">{{ name }}</h2>
      <p class="fcard__summary">{{ safeText(card.summary) }}</p>
      <div class="fcard__meta">
        <HTag tone="neutral" data-fiche-delivered>{{ delivered }}</HTag>
      </div>
    </div>
    <p v-if="!card.installable" class="fcard__note" data-fiche-not-installable>
      {{ t("catalogue.noServer") }}
    </p>
    <HButton
      class="fcard__open"
      variant="secondary"
      data-fiche-open
      :aria-label="t('catalogue.seeLabel', { name })"
      @click.stop="emit('open')"
    >
      {{ t("catalogue.see") }}
    </HButton>
  </article>
</template>

<style scoped>
.fcard {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: var(--space-3) var(--space-4);
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
  cursor: pointer;
}

.fcard:hover {
  background: var(--card-2);
}

.fcard--dim .fcard__text {
  opacity: var(--opacity-disabled);
}

.fcard__text {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.fcard__name {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.fcard__summary {
  margin: 0;
  color: var(--tx-secondary);
  overflow-wrap: anywhere;
}

.fcard__meta {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.fcard__note {
  grid-column: 1 / -1;
  margin: 0;
  color: var(--tx-secondary);
}

.fcard__open {
  grid-column: 2;
  grid-row: 1;
}
</style>
