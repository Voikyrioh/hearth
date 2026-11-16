<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import FicheVisual from "@/components/atoms/FicheVisual.vue";
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";
import { memorySentence } from "@/fiche/memory";
import { t } from "@/i18n";
import type { FicheCard } from "@/link";

// Une carte de la grille du Catalogue (HRT-241, BR-FICHE-003, 033, 063, 064) : l'image du jeu (ou son visuel neutre), son
// nom, un badge d'état, sa phrase, puis la mémoire conseillée en une phrase (fiche prête) ou la raison de son attente
// (fiche bientôt). Une carte prête s'ouvre entière (« Voir la fiche » reste le point d'entrée du clavier). Une carte
// bientôt n'a ni bouton, ni clic, ni survol cliquable : seule une construction d'essai la rend installable, et elle
// se comporte alors comme une carte prête. Les textes viennent de la fiche : montrés tels quels, jamais interprétés.
const props = defineProps<{
  card: FicheCard;
  serverId: string;
  serverName: string;
  /** La mémoire de la machine ; `null` tant qu'elle n'est pas lue. */
  capacityBytes: number | null;
}>();

const emit = defineEmits<{ open: [] }>();

const name = computed(() => safeText(props.card.name));
const soon = computed(() => props.card.status === "preview" && !props.card.installable);
const openable = computed(() => props.card.installable);
const memory = computed(() =>
  memorySentence(props.card.memory, props.capacityBytes, safeText(props.serverName), "card"),
);
const reason = computed(() =>
  props.card.previewReason ? safeText(props.card.previewReason) : null,
);

function open() {
  if (openable.value) emit("open");
}
</script>

<template>
  <article
    :class="['fcard', { 'fcard--soon': soon, 'fcard--open': openable }]"
    data-fiche-card
    :data-fiche="card.id"
    :data-soon="soon ? 'true' : undefined"
    @click="open"
  >
    <FicheVisual :server-id="serverId" :card="card" variant="card" class="fcard__visual" />
    <div class="fcard__body">
      <div class="fcard__head">
        <h3 class="fcard__name">{{ name }}</h3>
        <HTag :tone="soon ? 'warn' : 'ok'" data-fiche-badge>
          {{ soon ? t("catalogue.badgeSoon") : t("catalogue.badgeReady") }}
        </HTag>
      </div>
      <p class="fcard__summary">{{ safeText(card.summary) }}</p>
      <p v-if="soon && reason" class="fcard__line" data-fiche-reason>{{ reason }}</p>
      <p v-else-if="!soon" class="fcard__line" data-fiche-memory>{{ memory }}</p>
      <p v-if="!soon && !card.installable" class="fcard__line" data-fiche-not-installable>
        {{ t("catalogue.noServer") }}
      </p>
      <HButton
        v-if="openable"
        class="fcard__open"
        variant="secondary"
        data-fiche-open
        :aria-label="t('catalogue.seeLabel', { name })"
        @click.stop="emit('open')"
      >
        {{ t("catalogue.see") }}
      </HButton>
    </div>
  </article>
</template>

<style scoped>
.fcard {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.fcard--open {
  cursor: pointer;
}

.fcard--open:hover {
  background: var(--card-2);
}

/* Une carte « Bientôt » est atténuée et n'a aucun survol. */
.fcard--soon .fcard__visual {
  filter: saturate(0.4);
  opacity: var(--opacity-disabled);
}

.fcard__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4) var(--space-4);
  min-width: 0;
}

.fcard__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.fcard__name {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.fcard__summary,
.fcard__line {
  margin: 0;
  color: var(--tx-secondary);
  overflow-wrap: anywhere;
}

.fcard__line {
  font-size: var(--fs-small);
}

.fcard__open {
  align-self: flex-start;
}
</style>
