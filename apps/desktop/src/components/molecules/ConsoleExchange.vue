<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { t } from "@/i18n";
import { clockOf } from "@/logs/time";
import type { Exchange } from "@/services/console";

// Les commandes envoyées depuis CET écran et ce qui leur est arrivé (HRT-269, BR-CONSOLE-002, 004, 007, maquette Console) :
// la commande, l'heure, puis la réponse du jeu EN TEXTE BRUT (jamais du HTML, jamais interprétée), ou la phrase de
// l'interface : réponse vide, pas de réponse à temps (jamais renvoyée toute seule), commande refusée (ligne barrée, jamais
// envoyée), console injoignable. L'échange vit dans l'écran : l'agent ne le garde pas et un autre poste ne le voit pas.

const list = ref<HTMLElement | null>(null);
const props = defineProps<{ entries: readonly Exchange[] }>();
watch(
  () => props.entries.length,
  async () => {
    await nextTick();
    list.value?.scrollTo({ top: list.value.scrollHeight });
  },
);
</script>

<template>
  <section class="ce" :aria-label="t('console.exchange.label')" data-console-exchange>
    <p v-if="entries.length === 0" class="ce__empty" data-console-exchange-empty>
      {{ t("console.exchange.empty") }}
    </p>
    <ol v-else ref="list" class="ce__list" role="log" aria-live="polite">
      <li v-for="entry in entries" :key="entry.id" :class="['ce__item', `ce__item--${entry.outcome}`]" data-console-entry :data-outcome="entry.outcome">
        <p class="ce__command">
          <span class="ce__prompt" aria-hidden="true">&gt;</span>
          <span :class="['ce__text', { 'ce__text--barred': entry.outcome === 'refused' }]" data-console-command>{{ entry.command }}</span>
          <span class="ce__time">{{ clockOf(entry.at) }}</span>
        </p>
        <pre v-if="entry.reply" class="ce__reply" data-console-reply>{{ entry.reply }}</pre>
        <p v-if="entry.sentence" class="ce__sentence" data-console-sentence>{{ entry.sentence }}</p>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.ce {
  min-width: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.ce__empty {
  margin: 0;
  padding: var(--space-3) var(--space-4);
  color: var(--tx-secondary);
}

.ce__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  max-height: var(--log-min-height);
  margin: 0;
  padding: var(--space-3) var(--space-4);
  overflow-y: auto;
  list-style: none;
}

.ce__command {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  margin: 0;
  font-family: var(--font-mono);
}

.ce__prompt {
  color: var(--ac);
}

.ce__text {
  flex: 1 1 0;
  min-width: 0;
  overflow-wrap: anywhere;
}

.ce__text--barred {
  color: var(--tx-faint);
  text-decoration: line-through;
}

.ce__time {
  color: var(--tx-faint);
  font-size: var(--fs-small);
}

.ce__reply {
  margin: var(--space-1) 0 0;
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.ce__sentence {
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.ce__item--refused .ce__sentence,
.ce__item--no_answer .ce__sentence,
.ce__item--unreachable .ce__sentence {
  color: var(--warn);
}
</style>
