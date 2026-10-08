<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { burstLabel, capitalize, originOf, outcomeLabel, whenOf } from "@/audit/format";
import { displayRows } from "@/audit/rows";
import { clip, safeText } from "@/audit/text";
import HIcon from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import { t } from "@/i18n";
import type { AuditEntry } from "@/link";

// Tableau du journal (BR-AUDIT-002, 010, 013). Grille accessible (en-têtes, ligne active au
// clavier : flèches, Début, Fin, Page précédente et suivante ; Entrée ouvre le détail ou déploie
// une rafale), liste VIRTUALISÉE : une hauteur de ligne fixe, seules les lignes visibles existent dans
// le DOM, donc des milliers d'entrées restent fluides. Toute valeur est du texte non fiable :
// rendue par interpolation seulement (jamais en HTML), tronquée, caractères de contrôle neutralisés.
const props = defineProps<{
  entries: readonly AuditEntry[];
  /** Un chargement est en cours : les lignes restent, grisées. */
  busy: boolean;
  hasMore: boolean;
  loadingMore: boolean;
  /** Change quand la liste doit remonter en haut. */
  scrollSignal: number;
}>();

const emit = defineEmits<{ atTop: [value: boolean]; loadMore: []; open: [entry: AuditEntry] }>();

/** Lignes dessinées au-delà de la zone visible, de chaque côté. */
const OVERSCAN = 8;
/** Hauteur de repli (tests sans mise en page) quand la zone n'a pas de hauteur mesurable. */
const FALLBACK_VIEWPORT = 600;
const FALLBACK_ROW_HEIGHT = 44;

const scroller = ref<HTMLElement | null>(null);
const body = ref<HTMLElement | null>(null);
const frame = ref<HTMLElement | null>(null);
const expanded = ref<ReadonlySet<number>>(new Set());
const activeKey = ref<string | null>(null);
const range = ref({ start: 0, end: 40 });
let rowHeight = FALLBACK_ROW_HEIGHT;
let wasAtTop = true;
let observer: ResizeObserver | null = null;

const rows = computed(() => displayRows(props.entries, expanded.value, !props.hasMore));
const visible = computed(() =>
  rows.value.slice(range.value.start, range.value.end).map((row, offset) => ({
    row,
    index: range.value.start + offset,
  })),
);
const activeIndex = computed(() => {
  const found = rows.value.findIndex((row) => row.key === activeKey.value);
  return found < 0 ? 0 : found;
});

const COLUMNS = [
  "audit.colWhen",
  "audit.colAccount",
  "audit.colOrigin",
  "audit.colAction",
  "audit.colTarget",
  "audit.colOutcome",
  "audit.colReason",
] as const;

function readRowHeight() {
  const value = body.value
    ? Number.parseFloat(getComputedStyle(body.value).getPropertyValue("--audit-row-height"))
    : Number.NaN;
  rowHeight = Number.isFinite(value) && value > 0 ? value : FALLBACK_ROW_HEIGHT;
}

/** Recalcule les lignes à dessiner et pose les dimensions (propriétés CSS, jamais `style=`). */
function layout() {
  const el = scroller.value;
  if (!el) return;
  const viewport = el.clientHeight > 0 ? el.clientHeight : FALLBACK_VIEWPORT;
  const first = Math.max(0, Math.floor(el.scrollTop / rowHeight) - OVERSCAN);
  const last = Math.min(
    rows.value.length,
    Math.ceil((el.scrollTop + viewport) / rowHeight) + OVERSCAN,
  );
  range.value = { start: first, end: Math.max(last, first) };
  frame.value?.style.setProperty("--audit-rows-height", `${rows.value.length * rowHeight}px`);
  frame.value?.style.setProperty("--audit-offset", `${first * rowHeight}px`);
}

function onScroll() {
  layout();
  const el = scroller.value;
  if (!el) return;
  const top = el.scrollTop < rowHeight / 2;
  if (top !== wasAtTop) {
    wasAtTop = top;
    emit("atTop", top);
  }
  if (props.hasMore && !props.loadingMore && range.value.end >= rows.value.length - 5) {
    emit("loadMore");
  }
}

onMounted(() => {
  readRowHeight();
  layout();
  if (scroller.value && typeof ResizeObserver !== "undefined") {
    observer = new ResizeObserver(() => layout());
    observer.observe(scroller.value);
  }
});
onBeforeUnmount(() => observer?.disconnect());

watch(rows, () => {
  layout();
  // La liste a été remplie à la hauteur de la zone sans que l'utilisateur ait défilé : la suite se charge.
  void nextTick(() => {
    if (props.hasMore && !props.loadingMore && range.value.end >= rows.value.length - 5) {
      emit("loadMore");
    }
  });
});

watch(
  () => props.scrollSignal,
  () => {
    if (scroller.value) scroller.value.scrollTop = 0;
    wasAtTop = true;
    emit("atTop", true);
    layout();
  },
);

/** Déploie ou replie une rafale : tous ses membres entrent dans l'ensemble ou en sortent. */
function toggleBurst(row: Extract<(typeof rows.value)[number], { type: "burst" }>) {
  const next = new Set(expanded.value);
  for (const entry of row.burst.entries) {
    if (row.expanded) next.delete(entry.id);
    else next.add(entry.id);
  }
  expanded.value = next;
}

function summaryOf(index: number): string {
  const row = rows.value[index];
  return row && row.type === "burst" ? burstLabel(row.burst.attempts, row.burst.minutes) : "";
}

async function focusRow(index: number) {
  const target = rows.value[Math.max(0, Math.min(rows.value.length - 1, index))];
  const el = scroller.value;
  if (!target || !el) return;
  activeKey.value = target.key;
  const at = rows.value.indexOf(target);
  // Amène la ligne dans la zone visible avant de la chercher dans le DOM.
  const top = at * rowHeight;
  const viewport = el.clientHeight > 0 ? el.clientHeight : FALLBACK_VIEWPORT;
  if (top < el.scrollTop) el.scrollTop = top;
  else if (top + rowHeight > el.scrollTop + viewport) el.scrollTop = top + rowHeight - viewport;
  layout();
  await nextTick();
  el.querySelector<HTMLElement>(`[data-row-key="${target.key}"]`)?.focus();
}

function activate(index: number) {
  const row = rows.value[index];
  if (!row) return;
  activeKey.value = row.key;
  if (row.type === "burst") toggleBurst(row);
  else emit("open", row.entry);
}

function onKeydown(event: KeyboardEvent) {
  const index = activeIndex.value;
  switch (event.key) {
    case "ArrowDown":
      void focusRow(index + 1);
      break;
    case "ArrowUp":
      void focusRow(index - 1);
      break;
    case "PageDown":
      void focusRow(index + 10);
      break;
    case "PageUp":
      void focusRow(index - 10);
      break;
    case "Home":
      void focusRow(0);
      break;
    case "End":
      void focusRow(rows.value.length - 1);
      break;
    case "ArrowRight": {
      const row = rows.value[index];
      if (!(row && row.type === "burst" && !row.expanded)) return;
      toggleBurst(row);
      break;
    }
    case "ArrowLeft": {
      const row = rows.value[index];
      if (!(row && row.type === "burst" && row.expanded)) return;
      toggleBurst(row);
      break;
    }
    case "Enter":
    case " ":
      activate(index);
      break;
    default:
      return;
  }
  event.preventDefault();
}

function plain(value: string | null): string {
  return safeText(value);
}

defineExpose({ rows });
</script>

<template>
  <div class="table">
    <div
      class="table__grid"
      role="grid"
      :aria-label="t('audit.tableLabel')"
      :aria-rowcount="hasMore ? -1 : rows.length + 1"
      :aria-colcount="COLUMNS.length"
      :aria-busy="busy"
      @keydown="onKeydown"
    >
      <div class="table__head" role="row" aria-rowindex="1">
        <div v-for="key in COLUMNS" :key="key" class="table__th" role="columnheader">
          {{ t(key) }}
        </div>
      </div>
      <div ref="scroller" class="table__scroll" :class="{ 'table__scroll--busy': busy }" @scroll.passive="onScroll">
        <div ref="frame" class="table__frame">
          <div ref="body" class="table__window">
            <div
              v-for="{ row, index } in visible"
              :key="row.key"
              :data-row-key="row.key"
              :class="[
                'table__row',
                {
                  'table__row--burst': row.type === 'burst',
                  'table__row--child': row.type === 'entry' && row.child,
                },
              ]"
              role="row"
              :aria-rowindex="index + 2"
              :aria-expanded="row.type === 'burst' ? row.expanded : undefined"
              :tabindex="index === activeIndex ? 0 : -1"
              @click="activate(index)"
              @focus="activeKey = row.key"
            >
              <template v-if="row.type === 'burst'">
                <div class="table__burst" role="gridcell" :aria-colspan="COLUMNS.length">
                  <HIcon :name="row.expanded ? 'chevron-down' : 'chevron-right'" size="sm" />
                  <span class="table__burst-text">{{ burstLabel(row.burst.attempts, row.burst.minutes) }}</span>
                  <span v-if="row.burst.addr" class="table__muted">{{ t("audit.burstFrom", { addr: plain(row.burst.addr) }) }}</span>
                  <span class="table__sr">{{
                    t(row.expanded ? "audit.burstCollapse" : "audit.burstExpand", { summary: summaryOf(index) })
                  }}</span>
                </div>
              </template>
              <template v-else>
                <div class="table__td table__td--mono" role="gridcell" :title="whenOf(row.entry) + ' (UTC ' + plain(row.entry.at) + ')'">
                  <time :datetime="row.entry.at">{{ whenOf(row.entry) }}</time>
                </div>
                <div class="table__td" role="gridcell" :title="plain(row.entry.account)">
                  {{ clip(row.entry.account, 40).text }}
                </div>
                <div class="table__td" role="gridcell" :title="originOf(row.entry)">
                  {{ clip(originOf(row.entry)).text }}
                </div>
                <div class="table__td" role="gridcell" :title="plain(row.entry.actionLabel)">
                  {{ clip(row.entry.actionLabel).text }}
                </div>
                <div class="table__td" role="gridcell" :title="plain(row.entry.target)">
                  {{ clip(row.entry.target).text }}
                </div>
                <div class="table__td" role="gridcell">
                  <HTag :tone="row.entry.outcome === 'ok' ? 'ok' : 'crit'">
                    {{ outcomeLabel(row.entry.outcome) }}
                  </HTag>
                </div>
                <div class="table__td" role="gridcell" :title="plain(capitalize(row.entry.reason))">
                  {{ clip(capitalize(row.entry.reason)).text }}
                </div>
              </template>
            </div>
          </div>
        </div>
        <div v-if="loadingMore" class="table__more" role="status">
          <HSpinner :label="t('audit.loadingMore')" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* HRT-37 : le tableau prend la hauteur qui reste et défile lui-même ; la page ne défile pas (une seule barre). */
.table {
  display: flex;
  flex: 1 1 0;
  min-height: calc(var(--audit-row-height) * 5);
  overflow-x: auto;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.table__grid {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: var(--audit-table-min);
}

.table__head,
.table__row {
  display: grid;
  grid-template-columns: var(--audit-columns);
  align-items: center;
  gap: var(--space-3);
  padding: 0 var(--space-4);
}

.table__head {
  height: var(--audit-row-height);
  border-bottom: var(--border-width) solid var(--bd);
  color: var(--tx2);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  letter-spacing: var(--ls-label);
  text-transform: uppercase;
}

.table__scroll {
  position: relative;
  flex: 1 1 0;
  min-height: 0;
  overflow-y: auto;
}

.table__scroll--busy {
  opacity: var(--opacity-muted);
  pointer-events: none;
}

.table__frame {
  position: relative;
  height: var(--audit-rows-height, 0);
}

.table__window {
  position: absolute;
  top: var(--audit-offset, 0);
  right: 0;
  left: 0;
}

.table__row {
  height: var(--audit-row-height);
  border-bottom: var(--border-width) solid var(--bd);
  cursor: pointer;
}

.table__row:hover {
  background: var(--card-2);
}

.table__row:focus-visible {
  outline: var(--focus-ring) solid var(--tx);
  outline-offset: calc(-1 * var(--focus-ring));
}

.table__row--burst {
  background: var(--crit-tint);
}

.table__row--child {
  padding-left: var(--space-6);
}

.table__td {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.table__td--mono {
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.table__burst {
  display: flex;
  align-items: center;
  grid-column: 1 / -1;
  gap: var(--space-3);
  min-width: 0;
}

.table__burst-text {
  font-weight: var(--fw-semibold);
}

.table__muted {
  color: var(--tx2);
}

.table__sr {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.table__more {
  display: flex;
  justify-content: center;
  padding: var(--space-3);
}
</style>
