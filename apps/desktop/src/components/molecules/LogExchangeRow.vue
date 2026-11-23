<script setup lang="ts">
import { computed } from "vue";
import type { Entry } from "@/logs/buffer";
import { segmentsOf } from "@/logs/search";
import { clockOf } from "@/logs/time";

// Une commande envoyée depuis l'onglet « Console », rangée dans le journal à son heure (HRT-269, BR-CONSOLE-002, 004, 007,
// maquette Console) : la commande, l'heure, puis la réponse du jeu EN TEXTE BRUT (jamais du HTML, jamais interprétée), ou
// la phrase de l'interface (réponse vide, pas de réponse à temps, commande refusée : ligne barrée, jamais envoyée,
// console injoignable). La requête de recherche du journal y est surlignée comme dans une ligne du serveur.
const props = defineProps<{
  entry: Extract<Entry, { kind: "exchange" }>;
  query: string;
  active: boolean;
  current: boolean;
}>();

const command = computed(() => segmentsOf(props.entry.command, props.query));
const reply = computed(() =>
  props.entry.reply === null ? [] : segmentsOf(props.entry.reply, props.query),
);
</script>

<template>
  <div
    :class="['xr', `xr--${entry.outcome}`, { 'xr--active': active, 'xr--match': current }]"
    data-log-exchange
    data-console-entry
    :data-outcome="entry.outcome"
    :data-entry="entry.id"
  >
    <p class="xr__command">
      <span class="xr__prompt" aria-hidden="true">&gt;</span>
      <span :class="['xr__text', { 'xr__text--barred': entry.outcome === 'refused' }]" data-console-command><template v-for="(part, at) in command" :key="at"><mark v-if="part.hit" class="xr__hit">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></span>
      <span class="xr__time">{{ clockOf(entry.t) }}</span>
    </p>
    <pre v-if="entry.reply" class="xr__reply" data-console-reply><template v-for="(part, at) in reply" :key="at"><mark v-if="part.hit" class="xr__hit">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template></pre>
    <p v-if="entry.sentence" class="xr__sentence" data-console-sentence>{{ entry.sentence }}</p>
  </div>
</template>

<style scoped>
.xr {
  margin: var(--space-1) 0;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--card-2);
}

.xr--active {
  outline: var(--border-width) solid var(--ac);
}

.xr--match .xr__hit {
  outline: var(--border-width) solid var(--ac);
}

.xr--refused,
.xr--no_answer,
.xr--unreachable {
  box-shadow: inset var(--space-half) 0 0 var(--warn);
}

.xr__command {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  margin: 0;
  font-family: var(--font-mono);
}

.xr__prompt {
  color: var(--ac);
}

.xr__text {
  flex: 1 1 0;
  min-width: 0;
  overflow-wrap: anywhere;
}

.xr__text--barred {
  color: var(--tx-faint);
  text-decoration: line-through;
}

.xr__time {
  color: var(--tx-faint);
  font-size: var(--fs-small);
}

.xr__reply {
  margin: var(--space-1) 0 0;
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.xr__sentence {
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.xr--refused .xr__sentence,
.xr--no_answer .xr__sentence,
.xr--unreachable .xr__sentence {
  color: var(--warn);
}

.xr__hit {
  background: var(--warn-tint);
  color: inherit;
}
</style>
