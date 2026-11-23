<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import type { OnlinePlayersState } from "@/composables/useOnlinePlayers";
import { t } from "@/i18n";
import { ageText, hasPlayersCard, playersShown } from "@/services/players";

// La carte « Joueurs connectés » (HRT-291, BR-CONSOLE-011, maquette Connectes) : « N sur M places », les pseudos, l'âge de
// la lecture et « relire ». Elle est en tête de la vue d'ensemble d'un serveur Minecraft, dans l'onglet Console et sur
// l'écran « Vérifiez avant d'appliquer » des mods. Jamais un chiffre deviné : arrêté, qui démarre ou console muette, la
// carte dit une phrase fixe, sans chiffre. Un service sans console parlée n'a pas de carte. Les pseudos sont du texte du
// jeu, montrés tels quels (jamais du HTML). `compact` : la version courte des écrans qui l'accueillent (pas de phrase de
// cadence). `linkLabel` : le lien vers l'onglet Console, dit par l'écran qui accueille la carte.
const props = defineProps<{
  state: OnlinePlayersState;
  now: number;
  refreshing: boolean;
  compact?: boolean;
  linkLabel?: string;
}>();
const emit = defineEmits<{ refresh: []; openConsole: [] }>();

const view = computed(() => (props.state.kind === "ready" ? props.state.view : null));
const shown = computed(() => (view.value ? playersShown(view.value) : null));
const visible = computed(
  () =>
    props.state.kind === "loading" ||
    props.state.kind === "failed" ||
    (props.state.kind === "ready" && hasPlayersCard(view.value)),
);
const age = computed(() => ageText(view.value?.readAt ?? null, props.now));
const readLine = computed(() => {
  if (age.value === null) return null;
  return t(props.compact ? "players.readShort" : "players.read", { age: age.value });
});
</script>

<template>
  <DashCard v-if="visible" :title="t('players.title')" data-players-card>
    <template #aside>
      <HButton
        variant="ghost"
        size="sm"
        :busy="refreshing"
        :aria-label="t('players.refresh')"
        data-players-refresh
        @click="emit('refresh')"
      >
        <HIcon name="refresh" size="sm" />
      </HButton>
    </template>
    <HSkeleton v-if="state.kind === 'loading'" size="md" role="status" :aria-label="t('players.reading')" />
    <Callout v-else-if="state.kind === 'failed'" tone="warn" data-players-failed>
      {{ t("players.unavailable") }}
    </Callout>
    <template v-else-if="shown?.kind === 'count'">
      <p class="players__count" data-players-count>
        <template v-if="shown.online === 0">
          <strong class="players__figure">{{ t("players.nobody") }}</strong>{{ " " }}<span>{{ t("players.places", { max: shown.max }) }}</span>
        </template>
        <template v-else>
          <strong class="players__figure">{{ shown.online }}</strong>{{ " " }}<span>{{ t("players.outOf", { max: shown.max }) }}</span>
        </template>
      </p>
      <ul v-if="shown.names.length > 0" class="players__names" data-players-names>
        <li v-for="name in shown.names" :key="name" class="players__name">{{ name }}</li>
      </ul>
      <p v-if="shown.more > 0" class="players__note" data-players-more>
        {{ t("players.more", { n: shown.more }) }}
      </p>
      <p v-if="shown.hidden > 0" class="players__note" data-players-hidden>
        {{ t("players.hidden", { n: shown.hidden }) }}
      </p>
    </template>
    <p v-else-if="shown?.kind === 'sentence'" class="players__sentence" data-players-sentence>
      {{ shown.text }}
    </p>
    <p v-if="readLine && shown?.kind === 'count'" class="players__note" data-players-read>
      {{ readLine }}
    </p>
    <HButton
      v-if="linkLabel"
      variant="ghost"
      size="sm"
      class="players__link"
      data-players-console
      @click="emit('openConsole')"
    >
      {{ linkLabel }}
    </HButton>
  </DashCard>
</template>

<style scoped>
.players__count {
  margin: 0;
  color: var(--tx-secondary);
}

.players__figure {
  color: var(--tx);
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-bold);
}

.players__names {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.players__name {
  min-width: 0;
  max-width: 100%;
  padding: var(--space-half) var(--space-3);
  overflow: hidden;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.players__note,
.players__sentence {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.players__sentence {
  font-size: var(--fs-body);
}

.players__link {
  align-self: flex-start;
}
</style>
