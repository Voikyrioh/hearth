<script setup lang="ts">
import { computed } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";

// Base commune des bandeaux de sécurité (conception design, écran B) : un ton (`alert` : attaque
// probable, contour complet ambre ; `attack` : mode attaque actif, filet gauche rose ; `suspended` :
// mode suspendu, filet gauche turquoise), un pictogramme, un titre ÉCRIT (le sens ne repose jamais sur
// la couleur seule), un texte, des actions. Aucune logique : `tone` choisit le jeton et le pictogramme.
// `stamp` : « Dernier état connu à {heure} » quand le lien n'est pas « Connecté » (le bandeau ne
// s'estompe pas, il dit seulement ce qu'il sait). Seul le bandeau d'alerte est `role="alert"` (une
// attaque qui apparaît mérite d'être annoncée, une fois) ; les autres sont `role="status"`.
const props = defineProps<{
  tone: "alert" | "attack" | "suspended";
  title: string;
  stamp?: string;
}>();

const icon = computed(() =>
  props.tone === "alert" ? "alert" : props.tone === "attack" ? "shield" : "clock",
);
</script>

<template>
  <section
    :class="['banner', `banner--${tone}`]"
    :role="tone === 'alert' ? 'alert' : 'status'"
    :aria-label="title"
  >
    <HIcon class="banner__icon" :name="icon" />
    <div class="banner__body">
      <h2 class="banner__title">{{ title }}</h2>
      <p class="banner__text"><slot /></p>
      <p v-if="stamp" class="banner__stamp">{{ stamp }}</p>
    </div>
    <div class="banner__actions"><slot name="actions" /></div>
  </section>
</template>

<style scoped>
.banner {
  /* FIX:01M4D4FY4G650RBNJ3W7NSP947 : un bandeau tient sur une ligne, titre, texte et date côte à côte (revue UX C37) */
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-1) var(--space-4);
  border-radius: var(--radius-control);
  /* FIX:01M4DJZAPV5ECT10JJ5JWNM8A6 (C35) puis FIX:01M4EHYNPBB8TZ89DRRZ7TRH0A : le bandeau a la largeur du CONTENU de la page (même borne que la page,
     HRT-42), jamais celle d'une colonne de 880 px posée au-dessus d'une page plus large. */
}

.banner--alert {
  border: var(--border-width) solid var(--warn);
  background: var(--warn-tint);
}

.banner--attack {
  border-left: var(--sec-edge) solid var(--ac2);
  background: var(--sec-tint);
}

.banner--suspended {
  border-left: var(--sec-edge) solid var(--cool);
  background: var(--cool-tint);
}

.banner__icon {
  flex: none;
}

.banner--alert .banner__icon {
  color: var(--warn);
}

.banner--attack .banner__icon {
  color: var(--ac2);
}

.banner--suspended .banner__icon {
  color: var(--cool);
}

/* L'action suit le message (elle n'est plus reléguée à l'extrême droite de la fenêtre). */
.banner__body {
  display: flex;
  flex: 0 1 auto;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 var(--space-3);
  min-width: 0;
}

.banner__title {
  font-family: var(--font-title);
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.banner__text {
  color: var(--tx);
}

.banner__stamp {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.banner__actions {
  display: flex;
  flex: none;
  flex-wrap: nowrap;
  gap: var(--space-2);
}
</style>
