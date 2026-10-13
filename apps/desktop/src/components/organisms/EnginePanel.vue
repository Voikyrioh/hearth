<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon, { type IconName } from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import { useNow } from "@/composables/useNow";
import { type MessageKey, t } from "@/i18n";
import type { EngineInfo, EngineState } from "@/link";
import { type CountKey, type Counts, shownCounts } from "@/services/counts";
import { shortAge } from "@/services/format";
import { familyOf, helpFor } from "@/services/help";
import Callout from "../molecules/Callout.vue";
import CommandBlock from "../molecules/CommandBlock.vue";
import DashCard from "../molecules/DashCard.vue";
import MonoText from "../molecules/MonoText.vue";

// La carte « Moteur de conteneurs » (BR-ENGINE-001) : TOUJOURS le premier bloc de la section. Elle dit l'état
// du moteur par un mot ET une icône, ses compteurs à « Prêt », et pour chaque état non prêt son explication
// et son aide : du texte à recopier, adapté à la distribution (BR-ENGINE-005, 007, 008, 017). Aucun bouton
// d'installation, de réparation ni de démarrage (BR-ENGINE-006) ; le seul geste est « Vérifier à nouveau ».
// Les deux rôles voient le même état ; seul l'administrateur voit les blocs à recopier (BR-ENGINE-016).
// `engine` à `null` : première détection (« Vérification du moteur… », BR-ENGINE-019).
const props = defineProps<{
  engine: EngineInfo | null;
  counts: Counts | null;
  isAdmin: boolean;
  /** L'état du moteur avant l'actuel, pour « Dernier état connu » de « Ne répond pas ». */
  previous: { state: EngineState; at: number } | null;
  recheck: "idle" | "busy" | "failed";
}>();

const emit = defineEmits<{ recheck: []; docs: [] }>();

const now = useNow();

type Tone = "ok" | "warn" | "cool" | "plain";
interface Look {
  tone: Tone;
  icon: IconName;
  title: MessageKey;
}

const LOOKS: Record<Exclude<EngineState, "checking">, Look> = {
  ready: { tone: "ok", icon: "check", title: "engine.state.ready" },
  stopped: { tone: "warn", icon: "stop", title: "engine.state.stopped" },
  access_denied: { tone: "warn", icon: "lock", title: "engine.state.accessDenied" },
  unresponsive: { tone: "warn", icon: "clock", title: "engine.state.unresponsive" },
  absent: { tone: "cool", icon: "box", title: "engine.state.absent" },
  unsupported_engine: { tone: "warn", icon: "alert", title: "engine.state.unsupportedEngine" },
  unsupported_version: { tone: "warn", icon: "alert", title: "engine.state.unsupportedVersion" },
  disabled: { tone: "plain", icon: "pause", title: "engine.state.disabled" },
  unknown: { tone: "plain", icon: "question", title: "engine.state.unknown" },
};

const COUNT_ICONS: Record<CountKey, IconName> = {
  running: "play",
  stopped: "stop",
  error: "alert",
  transition: "clock",
  other: "question",
};

// Au singulier (n === 1), « arrêté » et « autre » s'accordent ; les trois autres catégories ne varient pas.
const COUNT_KEYS_ONE: Partial<Record<CountKey, MessageKey>> = {
  stopped: "engine.counters.stoppedOne",
  other: "engine.counters.otherOne",
};

const COUNT_KEYS: Record<CountKey, MessageKey> = {
  running: "engine.counters.running",
  stopped: "engine.counters.stopped",
  error: "engine.counters.error",
  transition: "engine.counters.transition",
  other: "engine.counters.other",
};

const look = computed(() =>
  props.engine && props.engine.state !== "checking" ? LOOKS[props.engine.state] : null,
);
const ready = computed(() => props.engine?.state === "ready");
const stateText = computed(() => (look.value ? t(look.value.title) : t("engine.checking")));
const counters = computed(() => (props.counts ? shownCounts(props.counts) : []));

/** L'aide existe seulement pour ces états, et l'explication demande à l'administrateur du serveur de régler. */
const needsAdminNote = computed(() => {
  const state = props.engine?.state;
  if (state === "stopped" || state === "absent") return true;
  return state === "access_denied" && props.engine?.agentIsAdmin === false;
});

const family = computed(() => (props.engine ? familyOf(props.engine.host) : null));
const declarative = computed(() => family.value === "nixos");
const distribution = computed(() => props.engine?.host.osId.trim() ?? "");

const help = computed(() => {
  const engine = props.engine;
  if (!engine) return null;
  switch (engine.state) {
    case "absent":
      return family.value === null ? ({ kind: "docs" } as const) : helpFor("install", engine);
    case "stopped":
      return helpFor("start", engine);
    case "access_denied":
      return engine.agentIsAdmin ? null : helpFor("access", engine);
    default:
      return null;
  }
});

const previousText = computed(() => {
  const before = props.previous;
  if (!before || before.state === "checking") return "";
  const label = LOOKS[before.state].title;
  return t("engine.lastKnown", { state: t(label), age: shortAge(now.value - before.at) });
});
const announce = computed(() => t("engine.announce", { state: stateText.value }));
</script>

<template>
  <DashCard
    :title="t('engine.title')"
    :class="['engine', look ? `engine--${look.tone}` : 'engine--plain']"
    :data-engine-state="engine?.state ?? 'checking'"
  >
    <!-- Annoncé une fois par changement d'état (l'agent ne change pas l'état plus d'une fois toutes les 5 s). -->
    <span class="engine__live" role="status" aria-live="polite">{{ announce }}</span>

    <div v-if="!look || !engine" class="engine__row" data-engine-checking>
      <HSpinner />
      <span class="engine__checking">{{ t("engine.checking") }}</span>
    </div>

    <template v-else-if="ready">
      <div class="engine__row">
        <HTag tone="ok" class="engine__ready"
          ><HIcon name="check" size="sm" /> {{ t("engine.ready") }}</HTag
        >
        <span v-if="engine.name" class="engine__name">{{
          t("engine.nameVersion", { name: engine.name, version: engine.version ?? "" }).trim()
        }}</span>
        <ul class="engine__counters" :aria-label="t('engine.counters.label')">
          <li v-for="item in counters" :key="item.key" :class="['count', `count--${item.key}`]">
            <HIcon :name="COUNT_ICONS[item.key]" size="sm" />
            {{ t((item.n === 1 && COUNT_KEYS_ONE[item.key]) || COUNT_KEYS[item.key], { n: item.n }) }}
          </li>
        </ul>
      </div>
    </template>

    <template v-else>
      <div class="engine__head">
        <HIcon class="engine__icon" :name="look.icon" />
        <p class="engine__state">{{ stateText }}</p>
      </div>

      <div class="engine__body">
        <!-- Moteur arrêté -->
        <template v-if="engine.state === 'stopped'">
          <p>{{ t("engine.stoppedText") }}</p>
          <p v-if="isAdmin && declarative">{{ t("engine.declarative") }}</p>
        </template>

        <!-- Accès refusé à l'agent -->
        <template v-else-if="engine.state === 'access_denied'">
          <p>
            <MonoText
              :text="engine.agentIsAdmin ? t('engine.deniedAdminText') : t('engine.deniedText')"
              :values="{ account: engine.agentAccount }"
            />
          </p>
          <Callout v-if="!engine.agentIsAdmin" tone="warn" inline>{{
            t("engine.deniedWarning")
          }}</Callout>
        </template>

        <!-- Ne répond pas -->
        <template v-else-if="engine.state === 'unresponsive'">
          <p>{{ t("engine.unresponsiveText") }}</p>
          <p v-if="previousText" class="engine__muted">{{ previousText }}</p>
        </template>

        <!-- Aucun moteur de conteneurs détecté -->
        <template v-else-if="engine.state === 'absent'">
          <p>{{ t("engine.absentText") }}</p>
          <p v-if="family !== null && distribution" class="engine__muted">
            <MonoText
              :text="t('engine.distribution')"
              :values="{ distribution }"
            />
          </p>
          <p v-if="family === null">{{ t("engine.unknownDistribution") }}</p>
          <p v-else-if="isAdmin && declarative">{{ t("engine.declarative") }}</p>
        </template>

        <!-- Moteur non pris en charge, version non prise en charge, pilotage désactivé -->
        <p v-else-if="engine.state === 'unsupported_engine'">
          <MonoText
            :text="t('engine.unsupportedEngineText')"
            :values="{ engine: engine.name ?? '' }"
          />
        </p>
        <p v-else-if="engine.state === 'unsupported_version'">
          <MonoText
            :text="t('engine.unsupportedVersionText')"
            :values="{ version: engine.version ?? '', minimum: engine.minVersion }"
          />
        </p>
        <p v-else-if="engine.state === 'disabled'">{{ t("engine.disabledText") }}</p>

        <!-- État annoncé par un agent plus récent que ce client : dit tel quel, sans deviner ni proposer d'aide. -->
        <p v-else-if="engine.state === 'unknown'" data-engine-unknown>{{ t("engine.unknownText") }}</p>

        <!-- L'aide : du texte à recopier (administrateur), ou le renvoi à la documentation, sans commande. -->
        <template v-if="help && help.kind === 'docs'">
          <p v-if="engine.state !== 'absent'">{{ t("engine.unknownDistribution") }}</p>
          <button type="button" class="engine__link" data-engine-docs @click="emit('docs')">
            {{ t("engine.docsLink") }}
          </button>
        </template>
        <div v-else-if="help && isAdmin" class="engine__help">
          <p class="engine__help-title">{{ help.title }}</p>
          <CommandBlock :text="help.text" :config="help.config" />
        </div>

        <Callout v-if="!isAdmin && needsAdminNote" tone="info" inline data-readonly-note>{{
          t("engine.readOnlyNote")
        }}</Callout>
      </div>

      <div class="engine__actions">
        <HButton
          variant="secondary"
          :busy="recheck === 'busy'"
          needs-link
          data-engine-recheck
          @click="emit('recheck')"
        >
          {{ t("engine.recheck") }}
        </HButton>
        <span v-if="recheck === 'busy'" class="engine__muted" role="status">{{
          t("engine.rechecking")
        }}</span>
      </div>
      <Callout v-if="recheck === 'failed'" tone="warn" inline>{{
        t("services.readFailed")
      }}</Callout>
    </template>
  </DashCard>
</template>

<style scoped>
.engine {
  min-height: var(--engine-card-min);
}

.engine.engine--warn {
  background: var(--warn-tint);
  box-shadow:
    var(--card-edge),
    inset var(--engine-edge) 0 0 var(--warn);
}

.engine.engine--cool {
  background: var(--cool-tint);
  box-shadow:
    var(--card-edge),
    inset var(--engine-edge) 0 0 var(--cool);
}

.engine__live {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.engine__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3) var(--space-5);
}

.engine__checking {
  color: var(--tx2);
}

.engine__ready {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.engine__name {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.engine__counters {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-4);
  padding: 0;
  margin: 0 0 0 auto;
  list-style: none;
}

.count {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1p5);
  color: var(--tx2);
  font-variant-numeric: tabular-nums;
}

.count--running {
  color: var(--ok);
}

.count--error {
  color: var(--crit);
}

.engine__head {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.engine--warn .engine__icon {
  color: var(--warn);
}

.engine--cool .engine__icon {
  color: var(--cool);
}

.engine--plain .engine__icon {
  color: var(--tx2);
}

.engine__state {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.engine__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.engine__body p {
  margin: 0;
  overflow-wrap: anywhere;
}

.engine__muted {
  color: var(--tx2);
}

.engine__help {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.engine__help-title {
  font-weight: var(--fw-semibold);
}

.engine__link {
  align-self: flex-start;
  padding: 0;
  border: 0;
  background: none;
  color: var(--ac);
  cursor: pointer;
  font: inherit;
  text-decoration: underline;
}

.engine__actions {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}
</style>
