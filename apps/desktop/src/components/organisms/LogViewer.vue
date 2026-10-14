<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useCountdown } from "@/composables/useCountdown";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import type { Entry } from "@/logs/buffer";
import { findMatches, segmentsOf } from "@/logs/search";
import { FOLD_CHARS, foldedText } from "@/logs/text";
import { clockOf, fullTimeOf } from "@/logs/time";
import { endNotice, exitWords, gapText, waitingText } from "@/logs/view";
import { atBottom, estimateHeight, layout, type Metrics, visibleRange } from "@/logs/windowing";
import { shortAge } from "@/services/format";
import { useServiceLogsStore } from "@/stores/serviceLogs";
import { useToastsStore } from "@/stores/toasts";
import HButton from "../atoms/HButton.vue";
import HIcon from "../atoms/HIcon.vue";
import HSkeleton from "../atoms/HSkeleton.vue";
import HSpinner from "../atoms/HSpinner.vue";
import HTag from "../atoms/HTag.vue";
import Callout from "../molecules/Callout.vue";
import EmptyState from "../molecules/EmptyState.vue";
import LogMarker from "../molecules/LogMarker.vue";
import LogSearch from "../molecules/LogSearch.vue";

// La visionneuse de journaux d'un service (HRT-67, ADR-0040, BR-SLOG-001 à 020). Trois idées :
//
// 1. Rien n'est réactif ligne par ligne. Le magasin tient un tampon ordinaire de 5 000 lignes au plus et émet un signal
//    (`version`) au plus toutes les 100 ms ; ici on ne dessine que la fenêtre visible (une soixantaine de nœuds), avec
//    des hauteurs estimées puis corrigées par la mesure. Mille lignes par seconde ne coûtent donc que dix mises à jour.
// 2. Rien d'une ligne n'est interprété : le texte est interpolé (jamais du HTML), ses séquences de terminal et ses
//    caractères de contrôle sont déjà neutralisés, aucun lien n'est fabriqué. La région est `aria-live="off"` : à ce
//    débit, annoncer les lignes noierait le lecteur d'écran.
// 3. Quitter l'onglet efface tout : le magasin ferme la lecture (la place est rendue au compte) et remplace le tampon.
const props = defineProps<{
  serverId: string;
  serviceKey: string;
  /** Le nom du service, déjà rendu inoffensif. */
  serviceName: string;
  /** Le lien avec le serveur est « Connecté ». */
  connected: boolean;
  lastContactAt: number | null;
  engineReady: boolean;
  /** Le service ne figure plus dans la liste. */
  gone: boolean;
  /** Le service n'existe pas encore (« Déploiement en cours »). */
  deploying: boolean;
}>();

const store = useServiceLogsStore();
const toasts = useToastsStore();
const now = useNow();
const countdown = useCountdown();

const scroller = ref<HTMLElement | null>(null);
const spacer = ref<HTMLElement | null>(null);
const rowsEl = ref<HTMLElement | null>(null);
const probeText = ref<HTMLElement | null>(null);
const probeChars = ref<HTMLElement | null>(null);

interface Row {
  entry: Entry;
  index: number;
}
const rows = shallowRef<Row[]>([]);
const expanded = ref<ReadonlySet<number>>(new Set());
const activeId = ref<number | null>(null);
const query = ref("");
const matches = shallowRef<{ indices: number[]; more: boolean }>({ indices: [], more: false });
const matchPos = ref(0);
const hasSelection = ref(false);

// ── Mesure et fenêtre ───────────────────────────────────────────────────────────────────────
const DEFAULT_VIEWPORT = 600;
let metrics: Metrics = { lineHeight: 18, columns: 100, padding: 2, marker: 32 };
let tops: number[] = [];
let heights: number[] = [];
let list: Entry[] = [];
const measured = new Map<number, number>();
let lastTop = 0;
let frame = 0;

function shownChars(entry: Entry): number {
  if (entry.kind !== "line") return 0;
  return entry.text.length > FOLD_CHARS && !expanded.value.has(entry.id)
    ? FOLD_CHARS
    : entry.text.length;
}

function extraLines(entry: Entry): number {
  if (entry.kind !== "line") return 0;
  return (entry.cut !== null ? 1 : 0) + (entry.text.length > FOLD_CHARS ? 1 : 0);
}

function heightOf(entry: Entry): number {
  return (
    measured.get(entry.id) ?? estimateHeight(entry, metrics, shownChars(entry), extraLines(entry))
  );
}

/** Lit la taille d'un caractère et la largeur d'une ligne dans la page réelle (police chargée, largeur courante). */
function measureMetrics(): boolean {
  const text = probeText.value;
  const chars = probeChars.value;
  if (!text || !chars) return false;
  const charWidth = chars.getBoundingClientRect().width / 100;
  const lineHeight = chars.getBoundingClientRect().height;
  const width = text.clientWidth;
  if (!(charWidth > 0) || !(lineHeight > 0) || !(width > 0)) return false;
  const columns = Math.max(1, Math.floor(width / charWidth));
  const marker = Number.parseFloat(
    getComputedStyle(scroller.value ?? document.body).getPropertyValue("--log-marker-height"),
  );
  const changed = columns !== metrics.columns || Math.abs(lineHeight - metrics.lineHeight) > 0.1;
  metrics = {
    lineHeight,
    columns,
    padding: metrics.padding,
    marker: Number.isFinite(marker) && marker > 0 ? marker : metrics.marker,
  };
  if (changed) measured.clear();
  return changed;
}

function rebuild(): void {
  list = store.entries();
  if (measured.size > list.length * 2 + 200) {
    const alive = new Set(list.map((entry) => entry.id));
    for (const id of measured.keys()) if (!alive.has(id)) measured.delete(id);
  }
  heights = list.map(heightOf);
  const placed = layout(heights);
  tops = placed.tops;
  syncWindow(placed.total);
}

function syncWindow(
  total = tops.length === 0 ? 0 : (tops[tops.length - 1] ?? 0) + (heights[heights.length - 1] ?? 0),
) {
  const el = scroller.value;
  const viewport = el?.clientHeight || DEFAULT_VIEWPORT;
  const [first, last] = visibleRange(tops, heights, el?.scrollTop ?? 0, viewport, metrics);
  const next: Row[] = [];
  for (let i = first; i < last; i += 1) {
    const entry = list[i];
    if (entry) next.push({ entry, index: i });
  }
  rows.value = next;
  spacer.value?.style.setProperty("--log-total", `${total}px`);
  rowsEl.value?.style.setProperty("--log-offset", `${tops[first] ?? 0}px`);
}

/** Corrige les hauteurs estimées par celles que le navigateur a vraiment dessinées (trois passes au plus). */
async function settle(): Promise<void> {
  for (let pass = 0; pass < 3; pass += 1) {
    await nextTick();
    const host = rowsEl.value;
    if (!host) return;
    let changed = false;
    for (const element of host.querySelectorAll<HTMLElement>("[data-log-row], [data-log-marker]")) {
      const id = Number(element.dataset.entry);
      const height = element.getBoundingClientRect().height;
      if (!(height > 0) || !Number.isFinite(id)) continue;
      const known = measured.get(id);
      if (known === undefined || Math.abs(known - height) > 0.5) {
        measured.set(id, height);
        changed = true;
      }
    }
    if (!changed) return;
    rebuild();
  }
}

function toBottom(): void {
  const el = scroller.value;
  if (!el) return;
  el.scrollTop = el.scrollHeight;
  lastTop = el.scrollTop;
}

async function refresh(): Promise<void> {
  if (measureMetrics()) measured.clear();
  rebuild();
  recomputeMatches();
  await settle();
  if (store.mode === "live") toBottom();
  syncWindow();
}

function onScroll(): void {
  const el = scroller.value;
  if (!el) return;
  const top = el.scrollTop;
  const movedUp = top < lastTop - 0.5;
  lastTop = top;
  const bottom = atBottom(top, el.clientHeight, el.scrollHeight);
  if (store.mode === "live") {
    // Remonter pose la pause (BR-SLOG-005) ; descendre ne fait jamais rien de plus.
    if (movedUp && !bottom) store.pause("auto");
  } else if (store.mode === "auto" && bottom) {
    // Redescendre tout en bas reprend le direct sans clic.
    store.resume();
  }
  if (!frame) {
    frame = requestAnimationFrame(() => {
      frame = 0;
      syncWindow();
    });
  }
}

watch(
  () => store.version,
  () => void refresh(),
);

// ── Cycle de vie : une seule lecture, ouverte avec l'onglet, fermée en le quittant ──────────
let resizer: ResizeObserver | undefined;

function openReading(): void {
  // Un service qui n'existe pas encore ou plus : rien à lire (le magasin garde seulement ce qu'il a déjà reçu).
  if (props.deploying || props.gone) return;
  void store.open(props.serverId, props.serviceKey);
}

function onSelectionChange(): void {
  const selection = document.getSelection();
  hasSelection.value =
    !!selection &&
    !selection.isCollapsed &&
    !!scroller.value &&
    scroller.value.contains(selection.anchorNode);
}

function onHide(): void {
  // Fenêtre fermée ou rechargée : rien ne reste en mémoire.
  store.close();
}

onMounted(() => {
  openReading();
  document.addEventListener("selectionchange", onSelectionChange);
  window.addEventListener("pagehide", onHide);
  void document.fonts?.ready.then(() => void refresh());
  void refresh();
});

// La fenêtre de lignes n'existe qu'une fois la lecture ouverte : on l'observe dès qu'elle apparaît.
if (typeof ResizeObserver !== "undefined") resizer = new ResizeObserver(() => void refresh());
watch(
  scroller,
  (element) => {
    resizer?.disconnect();
    if (!element) return;
    resizer?.observe(element);
    void refresh();
  },
  { flush: "post" },
);

onBeforeUnmount(() => {
  document.removeEventListener("selectionchange", onSelectionChange);
  window.removeEventListener("pagehide", onHide);
  resizer?.disconnect();
  cancelAnimationFrame(frame);
  countdown.stop();
  // Quitter l'onglet, fermer le détail, changer de service : la lecture est fermée et les lignes sont effacées.
  store.close();
  measured.clear();
  list = [];
  rows.value = [];
});

watch(
  () => [props.serverId, props.serviceKey] as const,
  () => openReading(),
);
watch(
  () => props.deploying,
  (waiting, was) => {
    if (was && !waiting) openReading();
  },
);
watch(
  () => props.gone,
  (isGone) => {
    if (isGone) store.noteGone();
  },
  { immediate: true },
);

// Le frein de l'agent : le délai est rendu à l'utilisateur, jamais une relance en boucle.
watch(
  () => store.retryAt,
  (at) => {
    if (at === null) countdown.stop();
    else countdown.start((at - Date.now()) / 1000);
  },
  { immediate: true },
);

// ── Ce qui s'affiche ────────────────────────────────────────────────────────────────────────
const lines = computed(() => {
  void store.version;
  return store.entries();
});
const hasLines = computed(() => {
  void store.version;
  return store.entries().some((entry) => entry.kind === "line");
});

const notice = computed(() =>
  endNotice({
    end: store.end,
    failure: store.failure,
    hasLines: store.received,
    remainingS: countdown.remaining.value,
  }),
);
const status = computed(() => store.status);
const stopped = computed(() => status.value?.state === "ended");
const mixed = computed(() => status.value?.mixed === true);
const opening = computed(
  () => store.phase === "opening" || (store.phase === "reading" && status.value === null),
);
/** Le cadre de lignes : tant qu'une lecture est en cours, ou que des lignes sont encore à lire. */
const showFrame = computed(
  () =>
    !props.deploying &&
    !(props.gone && !store.received) &&
    (notice.value === null || store.received) &&
    !(status.value?.notKept && !store.received),
);
const empty = computed(
  () => store.phase === "reading" && status.value !== null && !hasLines.value && !store.paused,
);
const stale = computed(
  () => props.gone || (notice.value?.stale ?? false) || (!props.engineReady && hasLines.value),
);
const showBar = computed(() => showFrame.value && !props.deploying);

const stoppedCode = computed(() => {
  const found = status.value;
  if (!found || !stopped.value || found.exitCode === null || found.exitCode === 0) return null;
  const code = exitWords(found.exitCode);
  return found.endedAt === null
    ? t("logs.stoppedCodeNoAge", { code })
    : t("logs.stoppedCode", { code, age: shortAge(now.value - found.endedAt) });
});

const indicator = computed(() => {
  // Une lecture finie : l'avis dit tout, aucun état « en direct » ne reste affiché.
  if (store.end !== null || store.failure !== null) return null;
  if (!props.connected) {
    const age = props.lastContactAt === null ? null : shortAge(now.value - props.lastContactAt);
    return {
      icon: "clock" as const,
      text: age === null ? t("logs.interrupted") : t("logs.seen", { age }),
      tone: "faint",
    };
  }
  if (store.interrupted)
    return { icon: "refresh" as const, text: t("logs.interrupted"), tone: "faint" };
  if (store.paused) return { icon: "pause" as const, text: t("logs.paused"), tone: "calm" };
  const state = status.value?.state;
  if (state === "ended") return { icon: "stop" as const, text: t("logs.stopped"), tone: "calm" };
  if (state === "waiting") return { icon: "clock" as const, text: t("logs.waiting"), tone: "calm" };
  if (state === "live") return { icon: "live" as const, text: t("logs.live"), tone: "ok" };
  if (state === "unknown") {
    return { icon: "question" as const, text: t("logs.stateUnknown"), tone: "calm" };
  }
  return null;
});

const pauseLabel = computed(() => {
  if (store.mode === "manual") return t("logs.resumeLive");
  if (store.mode === "auto") return t("logs.backToLive");
  return t("logs.pause");
});

const regionLabel = computed(() => t("logs.region", { name: props.serviceName }));
const waitingLabel = computed(() => waitingText(store.waiting));
const floatingLabel = computed(() =>
  store.waiting > 0 ? t("logs.backToLiveCount", { n: waitingLabel.value }) : t("logs.backToLive"),
);

// ── Gestes ──────────────────────────────────────────────────────────────────────────────────
function onPauseButton(): void {
  if (store.mode === "live") store.pause("manual");
  else backToLive();
}

function backToLive(): void {
  store.resume();
  void nextTick().then(() => void refresh());
}

function toggleFold(id: number): void {
  const next = new Set(expanded.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expanded.value = next;
  measured.delete(id);
  rebuild();
  void settle();
}

function linesOnly(): number[] {
  const out: number[] = [];
  list.forEach((entry, index) => {
    if (entry.kind === "line") out.push(index);
  });
  return out;
}

function ensureVisible(index: number): void {
  const el = scroller.value;
  if (!el) return;
  const top = tops[index] ?? 0;
  const bottom = top + (heights[index] ?? 0);
  if (top < el.scrollTop) el.scrollTop = top;
  else if (bottom > el.scrollTop + el.clientHeight) el.scrollTop = bottom - el.clientHeight;
}

function moveActive(step: 1 | -1): void {
  const all = linesOnly();
  if (all.length === 0) return;
  const current = all.findIndex((index) => list[index]?.id === activeId.value);
  const nextAt = current < 0 ? (step === 1 ? 0 : all.length - 1) : current + step;
  const bounded = Math.min(all.length - 1, Math.max(0, nextAt));
  const index = all[bounded];
  if (index === undefined) return;
  activeId.value = list[index]?.id ?? null;
  ensureVisible(index);
  syncWindow();
}

function onKeydown(event: KeyboardEvent): void {
  const el = scroller.value;
  if (!el) return;
  if (event.target !== el) return;
  if (event.key === "ArrowUp" || event.key === "ArrowDown") {
    event.preventDefault();
    moveActive(event.key === "ArrowDown" ? 1 : -1);
  } else if (event.key === "Home") {
    event.preventDefault();
    el.scrollTop = 0;
  } else if (event.key === "End") {
    event.preventDefault();
    el.scrollTop = el.scrollHeight;
  }
}

const activeLine = computed(() => {
  void store.version;
  const found = lines.value.find((entry) => entry.id === activeId.value);
  if (found?.kind !== "line") return null;
  const when = fullTimeOf(found.t);
  return t("logs.activeLine", { time: when ?? t("logs.timeUnknown") });
});

function recomputeMatches(): void {
  matches.value = findMatches(list, query.value);
  if (matchPos.value >= matches.value.indices.length) matchPos.value = 0;
}

watch(query, () => {
  matchPos.value = 0;
  // Les positions sont relues avant de chercher : une ligne arrivée il y a moins de 100 ms y est déjà.
  rebuild();
  recomputeMatches();
  if (matches.value.indices.length > 0) goToMatch(0);
});

function goToMatch(position: number): void {
  rebuild();
  recomputeMatches();
  const total = matches.value.indices.length;
  if (total === 0) return;
  matchPos.value = ((position % total) + total) % total;
  const index = matches.value.indices[matchPos.value];
  if (index === undefined) return;
  const el = scroller.value;
  if (!el) return;
  activeId.value = list[index]?.id ?? null;
  el.scrollTop = Math.max(0, (tops[index] ?? 0) - el.clientHeight / 3);
  syncWindow();
}

const currentMatchId = computed(() => {
  const index = matches.value.indices[matchPos.value];
  return index === undefined ? null : (list[index]?.id ?? null);
});

async function copySelection(): Promise<void> {
  const selection = document.getSelection();
  const text = selection && !selection.isCollapsed ? selection.toString() : "";
  if (text === "") {
    toasts.push({ kind: "info", message: t("logs.copyNothing"), key: "logs-copy" });
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    toasts.push({ kind: "success", message: t("logs.copied"), key: "logs-copy" });
  } catch {
    // Jamais le texte copié dans un message d'erreur.
    toasts.push({ kind: "error", message: t("logs.copyFailed"), key: "logs-copy" });
  }
}

function segments(row: Row) {
  const entry = row.entry;
  if (entry.kind !== "line") return [];
  const shown =
    entry.text.length > FOLD_CHARS && !expanded.value.has(entry.id)
      ? foldedText(entry.text)
      : entry.text;
  return segmentsOf(shown, query.value);
}

function restartText(entry: Extract<Entry, { kind: "restart" }>): string {
  if (entry.at === null) return t("logs.restartedUnknown");
  const time = clockOf(entry.at);
  return entry.count > 1
    ? t("logs.restartedMany", { n: entry.count, time })
    : t("logs.restarted", { time });
}

function retry(): void {
  void store.retry();
}

const retrying = computed(() => store.phase === "opening");
</script>

<template>
  <section class="logs" :data-logs-phase="store.phase" data-logs>
    <!-- Permanent, non fermable, dans tous les états (BR-SLOG-018) -->
    <Callout tone="info" inline data-logs-notice>{{ t("logs.notice") }}</Callout>

    <!-- Le service n'existe pas encore (BR-SLOG-023) -->
    <Callout v-if="deploying" tone="info" data-logs-deploying>{{ t("logs.deploying") }}</Callout>

    <!-- Les avis : lecture refusée, finie ou impossible. Les lignes déjà reçues restent en dessous. -->
    <Callout
      v-if="notice && !deploying"
      :tone="notice.tone"
      :role="notice.alert ? 'alert' : 'status'"
      data-logs-notice-end
      :data-end="store.end?.kind === 'refused' ? store.end.refusal : (store.end?.kind ?? store.failure ?? undefined)"
    >
      <HIcon v-if="notice.lock" name="lock" size="sm" />
      {{ notice.text }}
      <template v-if="notice.retry !== 'none'" #actions>
        <HButton
          variant="secondary"
          size="sm"
          :busy="retrying"
          :disabled="notice.retry === 'wait' && countdown.remaining.value > 0"
          :needs-link="true"
          data-logs-retry
          @click="retry"
        >
          {{ retrying ? t("logs.retrying") : t("logs.retry") }}
        </HButton>
      </template>
    </Callout>

    <Callout v-if="status?.notKept && !store.received" tone="info" data-logs-not-kept>
      {{ t("logs.notKept") }}
    </Callout>

    <template v-if="showBar && stopped && !(status?.notKept && !store.received)">
      <Callout tone="info" data-logs-stopped>{{ t("logs.stoppedBanner") }}</Callout>
      <Callout v-if="stoppedCode" tone="warn" data-logs-stopped-code>{{ stoppedCode }}</Callout>
    </template>
    <Callout v-if="mixed && showFrame" tone="info" inline data-logs-mixed>
      {{ t("logs.mixed") }}
    </Callout>
    <Callout v-if="!engineReady && hasLines && !notice" tone="warn" data-logs-engine>
      {{ t("logs.engineStale") }}
    </Callout>

    <!-- La barre d'état et de commande -->
    <div v-if="showBar && !opening" class="bar" data-logs-bar>
      <p v-if="indicator" class="bar__state" role="status" :data-tone="indicator.tone" data-logs-state>
        <span v-if="indicator.icon === 'live'" class="bar__dot" aria-hidden="true" />
        <HIcon v-else :name="indicator.icon" size="sm" />
        {{ indicator.text }}
      </p>
      <span v-if="store.paused" class="bar__count" data-logs-waiting>{{ waitingLabel }}</span>
      <span class="bar__space" />
      <LogSearch
        v-model="query"
        :current="matches.indices.length === 0 ? 0 : matchPos + 1"
        :total="matches.indices.length"
        :more="matches.more"
        @next="goToMatch(matchPos + 1)"
        @previous="goToMatch(matchPos - 1)"
      />
      <HButton
        variant="secondary"
        size="sm"
        :disabled="!hasSelection"
        data-logs-copy
        @click="copySelection"
      >
        <HIcon name="copy" size="sm" />
        {{ t("logs.copy") }}
      </HButton>
      <HButton variant="secondary" size="sm" :needs-link="true" data-logs-pause @click="onPauseButton">
        <HIcon :name="store.mode === 'live' ? 'pause' : 'arrow-down'" size="sm" />
        {{ pauseLabel }}
      </HButton>
    </div>

    <!-- Chargement : des lignes esquissées, jamais une fausse liste -->
    <div
      v-if="showFrame && opening && !deploying"
      class="loading"
      role="status"
      aria-busy="true"
      :aria-label="t('logs.reading')"
      data-logs-loading
    >
      <p class="loading__label"><HSpinner />{{ t("logs.reading") }}</p>
      <HSkeleton v-for="n in 12" :key="n" :size="n % 3 === 0 ? 'sm' : n % 2 === 0 ? 'md' : 'lg'" />
    </div>

    <!-- La fenêtre de lignes -->
    <div v-if="showFrame && !opening" class="frame" :data-stale="stale ? '' : undefined" :data-mixed="mixed ? '' : undefined">
      <div
        ref="scroller"
        class="scroller"
        role="region"
        tabindex="0"
        aria-live="off"
        :aria-label="regionLabel"
        :aria-describedby="`${serviceKey}-logs-hint`"
        data-logs-scroller
        @scroll.passive="onScroll"
        @keydown="onKeydown"
      >
        <div ref="spacer" class="spacer">
          <div ref="rowsEl" class="rows" data-logs-rows>
            <template v-for="row in rows" :key="row.entry.id">
              <LogMarker
                v-if="row.entry.kind === 'restart'"
                kind="restart"
                :text="restartText(row.entry)"
                :data-entry="row.entry.id"
              />
              <LogMarker
                v-else-if="row.entry.kind === 'gap'"
                kind="gap"
                :text="gapText(row.entry.gap, row.entry.count)"
                :data-entry="row.entry.id"
              />
              <div
                v-else
                :class="[
                  'row',
                  {
                    'row--err': row.entry.source === 'err' && !mixed,
                    'row--active': row.entry.id === activeId,
                    'row--match': row.entry.id === currentMatchId,
                  },
                ]"
                data-log-row
                :data-entry="row.entry.id"
                :data-source="row.entry.source"
                @click="activeId = row.entry.id"
              >
                <span class="row__time" :title="fullTimeOf(row.entry.t) ?? undefined">{{ clockOf(row.entry.t) }}</span>
                <span v-if="!mixed" class="row__tag">
                  <HTag v-if="row.entry.source === 'err'" tone="crit" :aria-label="t('logs.errorTagLabel')">
                    <HIcon name="alert" size="sm" />
                    {{ t("logs.errorTag") }}
                  </HTag>
                </span>
                <span class="row__text"><template v-for="(part, at) in segments(row)" :key="at"><mark v-if="part.hit" class="row__hit">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template><span v-if="row.entry.text.length > FOLD_CHARS && !expanded.has(row.entry.id)" class="row__more">…</span><span v-if="row.entry.cut !== null" class="row__cut">{{ t("logs.cutSuffix", { n: row.entry.cut }) }}</span></span>
                <button
                  v-if="row.entry.text.length > FOLD_CHARS"
                  type="button"
                  class="row__fold"
                  data-log-fold
                  @click.stop="toggleFold(row.entry.id)"
                >
                  {{
                    expanded.has(row.entry.id)
                      ? t("logs.foldLess")
                      : t("logs.foldMore", { n: row.entry.text.length })
                  }}
                </button>
              </div>
            </template>
          </div>
          <!-- Pour mesurer la police et la largeur réelles : invisible, hors de la lecture d'écran -->
          <div class="row row--probe" aria-hidden="true">
            <span class="row__time">00:00:00</span>
            <span v-if="!mixed" class="row__tag" />
            <span ref="probeText" class="row__text"
              ><span ref="probeChars" class="probe__chars">{{ "0".repeat(100) }}</span></span
            >
          </div>
        </div>
        <div v-if="store.trimmed" class="trimmed" data-logs-trimmed>
          <p class="trimmed__text">{{ t("logs.trimmed") }}</p>
        </div>
        <EmptyState
          v-if="empty"
          class="empty"
          :title="t('logs.emptyTitle')"
          :text="t('logs.emptyText')"
          heading="h2"
          data-logs-empty
        />
      </div>
      <p :id="`${serviceKey}-logs-hint`" class="sr-only">{{ t("logs.regionHint") }}</p>
      <div v-if="store.paused" class="frame__float">
      <HButton
        variant="primary"
        :needs-link="true"
        data-logs-live
        @click="backToLive"
      >
        <HIcon name="arrow-down" size="sm" />
        {{ floatingLabel }}
      </HButton>
      </div>
      <p class="info" data-logs-info>{{ activeLine ?? "" }}</p>
    </div>
  </section>
</template>

<style scoped>
.logs {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  min-height: 0;
}

.bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.bar__state {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx);
  font-weight: var(--fw-semibold);
}

.bar__state[data-tone="faint"],
.bar__state[data-tone="calm"] {
  color: var(--tx-secondary);
}

.bar__dot {
  width: var(--space-2);
  height: var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--ok);
}

.bar__count {
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
}

.bar__space {
  flex: 1 1 0;
}

.loading {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--bg);
}

.loading__label {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
}

.frame {
  position: relative;
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  min-height: var(--log-min-height);
}

.frame[data-stale] .scroller {
  filter: grayscale(var(--grayscale-stale));
  opacity: var(--opacity-stale);
}

.scroller {
  position: relative;
  flex: 1 1 0;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overflow-anchor: none;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--bg);
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-log);
  line-height: var(--lh-log);
}

.scroller:focus-visible {
  outline: var(--space-half) solid var(--ac);
  outline-offset: calc(var(--space-half) * -1);
}

.spacer {
  position: relative;
  height: var(--log-total, 0);
  min-width: 0;
}

.rows {
  position: absolute;
  top: var(--log-offset, 0);
  right: 0;
  left: 0;
}

.row {
  display: grid;
  grid-template-columns: var(--log-time-width) var(--log-tag-width) minmax(0, 1fr);
  column-gap: var(--space-2);
  align-items: start;
  box-sizing: border-box;
  padding: var(--space-half) var(--space-3);
  border-left: var(--log-edge) solid transparent;
}

.frame[data-mixed] .row {
  grid-template-columns: var(--log-time-width) minmax(0, 1fr);
}

.row--err {
  border-left-color: var(--crit);
}

.row--active {
  background: var(--card-2);
}

.row--match .row__hit {
  outline: var(--border-width) solid var(--ac);
}

.row__time {
  color: var(--tx3);
  user-select: none;
}

.row__tag {
  user-select: none;
}

.row__tag :deep(.tag) {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.row__text {
  min-width: 0;
  white-space: pre-wrap;
  word-break: break-all;
  overflow-wrap: anywhere;
  user-select: text;
}

.row__hit {
  background: var(--warn-tint);
  color: inherit;
}

.row__cut,
.row__more {
  color: var(--tx3);
  user-select: none;
}

.row__fold {
  grid-column: 3 / -1;
  justify-self: start;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--ac);
  font: inherit;
  text-decoration: underline;
  cursor: pointer;
}

.frame[data-mixed] .row__fold {
  grid-column: 2 / -1;
}

.row--probe {
  position: absolute;
  right: 0;
  left: 0;
  visibility: hidden;
  pointer-events: none;
}

.probe__chars {
  white-space: pre;
}

/* Collant en haut de la fenêtre sans prendre de place : les positions des lignes ne bougent pas. */
.trimmed {
  position: sticky;
  top: 0;
  z-index: var(--z-raised);
  height: 0;
}

.trimmed__text {
  position: absolute;
  top: 0;
  right: 0;
  left: 0;
  margin: 0;
  padding: var(--space-1) var(--space-3);
  background: var(--bg);
  color: var(--tx3);
  font-family: var(--font-body);
  font-size: var(--fs-small);
}

.empty {
  position: absolute;
  inset: 0;
}

.frame__float {
  position: absolute;
  right: 0;
  bottom: var(--space-6);
  left: 0;
  display: flex;
  justify-content: center;
  pointer-events: none;
}

.frame__float > * {
  pointer-events: auto;
}

.info {
  min-height: var(--space-4);
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
