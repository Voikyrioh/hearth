<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import ConsoleCommandField from "@/components/molecules/ConsoleCommandField.vue";
import ConsoleExchange from "@/components/molecules/ConsoleExchange.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import PlayersOnlineCard from "@/components/molecules/PlayersOnlineCard.vue";
import ConsoleSensitiveDialog from "@/components/organisms/ConsoleSensitiveDialog.vue";
import LogViewer from "@/components/organisms/LogViewer.vue";
import { useNow } from "@/composables/useNow";
import { useOnlinePlayers } from "@/composables/useOnlinePlayers";
import { useServiceConsole } from "@/composables/useServiceConsole";
import { t } from "@/i18n";
import type { ConsoleView, QuickCommand, ServiceInfo } from "@/link";
import { canAct, type RightsGate } from "@/services/rights";

// L'onglet « Console » d'un serveur de jeu dont l'agent parle à la console (HRT-269, BR-CONSOLE-002 à 010, maquettes
// Console, ConsoleSensible, ConsoleEtats) : le journal du serveur en direct (la visionneuse existante, « Mettre en
// pause »), les commandes envoyées d'ici et leurs réponses, le champ « Commande », les commandes rapides de la fiche, la
// carte des joueurs connectés et ce que Hearth surveille. Lire l'onglet demande le droit de lire le service ; écrire
// demande le droit de gérer les services. Une commande refusée par la fiche n'est jamais envoyée ; une commande sensible
// demande le mot de passe à chaque fois. Tenu par `useServiceConsole`.
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  view: ConsoleView;
  gate: RightsGate;
  connected: boolean;
  engineReady: boolean;
  gone: boolean;
  lastContactAt: number | null;
}>();
const emit = defineEmits<{ refresh: []; start: []; follow: [] }>();

const console_ = useServiceConsole(
  () => props.serverId,
  () => props.service.key,
  () => props.view,
  () => emit("refresh"),
);
const clock = useNow();
const players = useOnlinePlayers(
  () => props.serverId,
  () => (props.connected && !props.gone ? props.service.key : null),
);
const name = computed(() => safeText(props.service.name));
const verbs = computed(() =>
  props.view.policy.sensitive.map((item) => `« ${safeText(item.verb)} »`),
);
const blocked = computed(() => console_.block.value !== null);
const canStart = computed(() => canAct(props.gate));

const field = ref<InstanceType<typeof ConsoleCommandField> | null>(null);

/** Une commande rapide « à compléter » pose son début : le curseur entre dans le champ, après lui. */
async function quick(command: QuickCommand): Promise<void> {
  await console_.quick(command);
  await nextTick();
  void field.value?.focus();
}

function refusalText(): string | undefined {
  return console_.refusal.value;
}
</script>

<template>
  <div class="ct" data-console-tab>
    <header class="ct__head">
      <h3 class="ct__title">{{ t("console.title") }}</h3>
      <p class="ct__lead">{{ t("console.lead") }}</p>
    </header>

    <div class="ct__grid">
      <div class="ct__main">
        <div class="ct__log" data-console-log>
          <LogViewer
          :server-id="serverId"
          :service-key="service.key"
          :service-name="name"
          :connected="connected"
          :last-contact-at="lastContactAt"
          :engine-ready="engineReady"
          :gone="gone"
          :deploying="service.state === 'deploying'"
          />
        </div>
        <ConsoleCommandField
          ref="field"
          v-model="console_.typed.value"
          :block="console_.block.value"
          :issue="console_.issue.value"
          :judged="console_.judged.value"
          :can-send="console_.canSend.value"
          :sending="console_.sending.value"
          :max-chars="console_.maxChars.value"
          :can-start="canStart"
          @send="console_.submit()"
          @start="emit('start')"
          @follow="emit('follow')"
        />
        <ConsoleExchange :entries="console_.exchanges.value" />
      </div>

      <aside class="ct__side">
        <PlayersOnlineCard
          :state="players.state.value"
          :now="clock"
          :refreshing="players.refreshing.value"
          compact
          @refresh="players.refresh()"
        />
        <DashCard :title="t('console.quick.title')" data-console-quick>
          <div class="ct__quick">
            <HButton
              v-for="command in view.quick"
              :key="command.id"
              variant="secondary"
              size="sm"
              needs-link
              :disabled="blocked"
              :data-console-quick-id="command.id"
              @click="quick(command)"
            >
              {{ command.label }}
            </HButton>
          </div>
          <p v-if="view.quick.some((item) => item.mode === 'fill')" class="ct__note">
            {{ t("console.quick.fill") }}
          </p>
        </DashCard>
        <DashCard :title="t('console.watch.title')" data-console-watch>
          <p v-if="verbs.length > 0" class="ct__note">
            {{ t("console.watch.sensitive", { verbs: verbs.join(", ") }) }}
          </p>
          <p class="ct__note">{{ t("console.watch.stop") }}</p>
          <p class="ct__note">{{ t("console.watch.journal") }}</p>
        </DashCard>
      </aside>
    </div>

    <ConsoleSensitiveDialog
      :server-id="serverId"
      :pending="console_.pending.value"
      :perform="console_.confirm"
      :refusal-text="refusalText"
      @close="console_.closePending()"
    />
  </div>
</template>

<style scoped>
.ct {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.ct__head {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.ct__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.ct__lead,
.ct__note {
  margin: 0;
  color: var(--tx-secondary);
}

.ct__note {
  font-size: var(--fs-small);
}

.ct__grid {
  display: grid;
  grid-template-columns: minmax(0, 2fr) minmax(var(--card-min), 1fr);
  gap: var(--space-4);
  align-items: start;
}

.ct__log {
  display: flex;
  flex-direction: column;
  height: var(--console-log-height);
  min-height: var(--log-min-height);
  min-width: 0;
}

.ct__main,
.ct__side {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.ct__quick {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}
</style>
