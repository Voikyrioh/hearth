<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HMiddleText from "@/components/atoms/HMiddleText.vue";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import { imageLabel, loopBand, PORTS_WIDTH, rowLabel, stateLine } from "@/services/format";
import { anchoredScroll, indexAt, offsetsOf, type RowHeights, windowOf } from "@/services/layout";
import OriginTags from "../molecules/OriginTags.vue";
import PortChips from "../molecules/PortChips.vue";
import PortsDialog from "../molecules/PortsDialog.vue";
import ServiceStateBadge from "../molecules/ServiceStateBadge.vue";

// La liste des services (BR-SVC-001, 002, 012, 015, 019, 021) : une grille ARIA (en-têtes de colonnes,
// `aria-rowcount`), triée par nom par la liaison (l'ordre ne dépend jamais de l'état), VIRTUALISÉE (seules les
// lignes visibles et quelques voisines sont dans le DOM, sans pagination). La hauteur d'une ligne se DÉDUIT de
// la donnée (standard, ou plus haute pour « Redémarre en boucle »), elle n'est jamais mesurée : le défilement
// ne saute donc pas quand une ligne change d'état, et une ligne qui apparaît ou change au-dessus de la zone
// visible laisse la ligne repère où elle était. Une ligne est reconnue par sa clé stable, jamais par son
// index : le focus clavier la suit. Un seul arrêt de tabulation (la ligne active, ou la première ligne visible
// quand la ligne active est sortie de la fenêtre dessinée : jamais aucun) ; flèches, Début, Fin et pages
// déplacent la ligne active ; un clic ou Entrée sur une ligne ouvre le détail du service (HRT-63, il emporte
// tous les ports) ; « +N » ouvre à la souris la fenêtre de tous ses ports. La ligne qui a le focus reste dessinée même hors de la zone visible (un défilement ne rend
// pas le focus au document). Tout texte est interpolé (jamais du HTML) ; un nom, une image de 255 caractères
// sont coupés au milieu et complets au survol. Le nom accessible d'une ligne donne tous ses ports.
const props = defineProps<{
  services: readonly ServiceInfo[];
  /** La clé du service dont le détail est ouvert (`aria-selected`), `null` : aucun. */
  selectedKey?: string | null;
}>();
const emit = defineEmits<{ open: [key: string] }>();

/** Lignes dessinées au-delà de la zone visible, de chaque côté. */
const OVERSCAN = 8;
/** Repli (tests sans mise en page) quand la zone n'a pas de hauteur mesurable. */
const FALLBACK_VIEWPORT = 600;
const FALLBACK_HEIGHTS: RowHeights = { row: 52, alert: 80 };
/** Largeur de la zone sous laquelle la liste passe en mode compact (même seuil que le `@container` plus bas). */
const COMPACT_BELOW = 960;
/** Largeur sous laquelle la liste est un volet à côté du détail : seulement Nom et État (la colonne Ports disparaît). */
const NARROW_BELOW = 600;

const now = useNow();
const root = ref<HTMLElement | null>(null);
const scroller = ref<HTMLElement | null>(null);
const frame = ref<HTMLElement | null>(null);
const heights = ref<RowHeights>({ ...FALLBACK_HEIGHTS });
const range = ref({ start: 0, end: 30 });
const activeKey = ref<string | null>(null);
/** Le focus clavier est dans la liste : la ligne qui l'a reste dessinée. */
const hasFocus = ref(false);
/** La première ligne visible (l'arrêt de tabulation quand la ligne active n'est pas dessinée). */
const firstVisible = ref(0);
/** Mode compact : la colonne « Image » disparaît, la colonne « Ports » est plus étroite. */
const compact = ref(false);
/** Mode volet (détail ouvert à côté) : deux colonnes. */
const narrow = ref(false);
const portsOf = ref<ServiceInfo | null>(null);
let observer: ResizeObserver | null = null;
let last: { keys: string[]; offsets: number[] } = { keys: [], offsets: [0] };

const keys = computed(() => props.services.map((service) => service.key));
const offsets = computed(() => offsetsOf(props.services, heights.value));
const visible = computed(() =>
  props.services
    .slice(range.value.start, range.value.end)
    .map((service, offset) => ({ service, index: range.value.start + offset })),
);
const activeIndex = computed(() => {
  const found = keys.value.indexOf(activeKey.value ?? "");
  return found < 0 ? 0 : found;
});
/** Une seule ligne est un arrêt de tabulation : la ligne active si elle est dessinée, sinon la première visible. */
const tabStop = computed(() => {
  const { start, end } = range.value;
  if (activeIndex.value >= start && activeIndex.value < end) return activeIndex.value;
  return Math.max(start, Math.min(end - 1, firstVisible.value));
});
const portsWidth = computed(() => (compact.value ? PORTS_WIDTH.compact : PORTS_WIDTH.wide));
const columnCount = computed(() =>
  narrow.value ? COLUMNS.length - 2 : compact.value ? COLUMNS.length - 1 : COLUMNS.length,
);

function readHeights() {
  if (!frame.value) return;
  const style = getComputedStyle(frame.value);
  const row = Number.parseFloat(style.getPropertyValue("--row-service"));
  const alert = Number.parseFloat(style.getPropertyValue("--row-service-alert"));
  if (row > 0 && alert > 0) heights.value = { row, alert };
}

/** Recalcule les lignes à dessiner et pose les dimensions (propriétés CSS, jamais `style=`). */
function layout() {
  const el = scroller.value;
  if (!el) return;
  const viewport = el.clientHeight > 0 ? el.clientHeight : FALLBACK_VIEWPORT;
  const width = root.value?.clientWidth ?? 0;
  compact.value = width > 0 && width <= COMPACT_BELOW;
  narrow.value = width > 0 && width <= NARROW_BELOW;
  let window = windowOf(offsets.value, el.scrollTop, viewport, OVERSCAN);
  firstVisible.value = indexAt(offsets.value, el.scrollTop);
  if (hasFocus.value) {
    const focused = keys.value.indexOf(activeKey.value ?? "");
    if (focused >= 0) {
      window = { start: Math.min(window.start, focused), end: Math.max(window.end, focused + 1) };
    }
  }
  range.value = window;
  frame.value?.style.setProperty("--svc-total", `${offsets.value.at(-1) ?? 0}px`);
  frame.value?.style.setProperty("--svc-offset", `${offsets.value[window.start] ?? 0}px`);
  last = { keys: [...keys.value], offsets: [...offsets.value] };
}

onMounted(() => {
  readHeights();
  layout();
  if (scroller.value && typeof ResizeObserver !== "undefined") {
    observer = new ResizeObserver(() => layout());
    observer.observe(scroller.value);
    if (root.value) observer.observe(root.value);
  }
});
onBeforeUnmount(() => observer?.disconnect());

// La liste change : la ligne repère reste où elle était (BR-SVC-015), puis on redessine la fenêtre.
watch(
  () => props.services,
  () => {
    const el = scroller.value;
    if (!el) return;
    const next = { keys: keys.value, offsets: offsets.value };
    const target = anchoredScroll(last, next, el.scrollTop);
    // Les dimensions d'abord (la hauteur totale change), le défilement ensuite.
    frame.value?.style.setProperty("--svc-total", `${offsets.value.at(-1) ?? 0}px`);
    if (target !== el.scrollTop) el.scrollTop = target;
    layout();
  },
);

function onScroll() {
  layout();
}

async function focusRow(index: number) {
  const target = props.services[Math.max(0, Math.min(props.services.length - 1, index))];
  const el = scroller.value;
  if (!target || !el) return;
  activeKey.value = target.key;
  const at = keys.value.indexOf(target.key);
  const top = offsets.value[at] ?? 0;
  const bottom = offsets.value[at + 1] ?? top;
  const viewport = el.clientHeight > 0 ? el.clientHeight : FALLBACK_VIEWPORT;
  if (top < el.scrollTop) el.scrollTop = top;
  else if (bottom > el.scrollTop + viewport) el.scrollTop = bottom - viewport;
  layout();
  await nextTick();
  const row = [...el.querySelectorAll<HTMLElement>("[data-row]")].find(
    (node) => node.dataset.key === target.key,
  );
  row?.focus();
}

function onFocusOut(event: FocusEvent) {
  const grid = event.currentTarget;
  if (
    grid instanceof HTMLElement &&
    event.relatedTarget instanceof Node &&
    grid.contains(event.relatedTarget)
  ) {
    return;
  }
  hasFocus.value = false;
  layout();
}

function onKeydown(event: KeyboardEvent) {
  // Entrée (ou Espace) sur la LIGNE, pas sur un contrôle dedans : le détail du service s'ouvre.
  const target = event.target;
  if (
    (event.key === "Enter" || event.key === " ") &&
    target instanceof HTMLElement &&
    target.dataset.row !== undefined
  ) {
    const key = target.dataset.key;
    if (key) emit("open", key);
    event.preventDefault();
    return;
  }
  const index = activeIndex.value;
  const page = Math.max(
    1,
    Math.floor((scroller.value?.clientHeight || FALLBACK_VIEWPORT) / heights.value.row) - 1,
  );
  switch (event.key) {
    case "ArrowDown":
      void focusRow(index + 1);
      break;
    case "ArrowUp":
      void focusRow(index - 1);
      break;
    case "PageDown":
      void focusRow(index + page);
      break;
    case "PageUp":
      void focusRow(index - page);
      break;
    case "Home":
      void focusRow(0);
      break;
    case "End":
      void focusRow(props.services.length - 1);
      break;
    default:
      return;
  }
  event.preventDefault();
}

const COLUMNS = [
  "services.colName",
  "services.colImage",
  "services.colState",
  "services.colPorts",
] as const;

/** Rend le focus à la ligne d'un service (retour du détail), même si elle n'est pas dessinée. */
async function focusKey(key: string) {
  const index = keys.value.indexOf(key);
  if (index >= 0) await focusRow(index);
}

defineExpose({ visible, focusKey });
</script>

<template>
  <div ref="root" class="table">
    <div
      class="table__grid"
      role="grid"
      :aria-label="t('services.listLabel')"
      :aria-rowcount="services.length + 1"
      :aria-colcount="columnCount"
      @keydown="onKeydown"
      @focusin="hasFocus = true"
      @focusout="onFocusOut"
    >
      <div class="table__head" role="row" aria-rowindex="1">
        <div
          v-for="(key, column) in COLUMNS"
          :key="key"
          :class="['table__th', { 'table__th--image': column === 1, 'table__th--ports': column === 3 }]"
          role="columnheader"
        >
          {{ t(key) }}
        </div>
      </div>
      <div ref="scroller" class="table__scroll" @scroll.passive="onScroll">
        <div ref="frame" class="table__frame">
          <div class="table__window">
            <div
              v-for="{ service, index } in visible"
              :key="service.key"
              data-row
              :data-key="service.key"
              :data-state="service.state"
              :class="['row', { 'row--alert': service.state === 'restart_loop' }]"
              role="row"
              :aria-rowindex="index + 2"
              :aria-label="rowLabel(service)"
              :aria-selected="selectedKey === service.key"
              :tabindex="index === tabStop ? 0 : -1"
              @focus="activeKey = service.key"
              @click="emit('open', service.key)"
            >
              <div class="cell cell--name" role="gridcell">
                <span class="name-line">
                  <HMiddleText
                    class="name"
                    :text="service.name"
                    :focusable="false"
                    preserve-spaces
                  />
                  <OriginTags :service="service" part="origin" />
                </span>
                <span class="detail-line">
                  <OriginTags :service="service" part="details" />
                  <HMiddleText
                    class="image image--inline"
                    :text="imageLabel(service)"
                    :focusable="false"
                    preserve-spaces
                  />
                </span>
              </div>
              <div class="cell cell--image" role="gridcell">
                <HMiddleText
                  class="image"
                  :text="imageLabel(service)"
                  :focusable="false"
                  preserve-spaces
                />
              </div>
              <div class="cell cell--state" role="gridcell">
                <ServiceStateBadge :state="service.state" />
                <span class="state-line">{{ stateLine(service, now) }}</span>
              </div>
              <div class="cell cell--ports" role="gridcell">
                <PortChips
                  :ports="service.ports"
                  :omitted="service.portsOmitted"
                  :host-network="service.hostNetwork"
                  :width="portsWidth"
                  @more="portsOf = service"
                />
              </div>
              <div
                v-if="service.state === 'restart_loop'"
                class="band"
                role="gridcell"
                :aria-colspan="columnCount"
              >
                <HIcon name="alert" size="sm" />
                <span>{{ loopBand(service) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
    <PortsDialog :service="portsOf" @close="portsOf = null" />
  </div>
</template>

<style scoped>
.table {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  min-height: calc(var(--row-service) * 4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
  container: svctable / inline-size;
}

.table__grid {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
}

.table__head,
.row {
  display: grid;
  grid-template-columns: var(--svc-columns);
  align-items: center;
  gap: var(--space-3);
  padding: 0 var(--space-4);
}

.table__head {
  /* Même gouttière que la zone de lignes (barre de défilement classique), sinon les en-têtes se décalent. */
  overflow: hidden;
  scrollbar-gutter: stable;
  height: var(--control-sm);
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
  overflow-x: hidden;
  overflow-y: auto;
  scrollbar-gutter: stable;
}

.table__frame {
  position: relative;
  height: var(--svc-total, 0);
}

.table__window {
  position: absolute;
  top: var(--svc-offset, 0);
  right: 0;
  left: 0;
}

.row {
  grid-template-rows: var(--row-service) auto;
  align-content: start;
  height: var(--row-service);
  border-bottom: var(--border-width) solid var(--bd);
}

.row--alert {
  height: var(--row-service-alert);
}

.row {
  cursor: pointer;
}

.row[aria-selected="true"],
.row:hover,
.row:focus-visible {
  background: var(--card-2);
  box-shadow: inset var(--svc-puck) 0 0 var(--ac);
}

.row:focus-visible {
  outline: var(--focus-ring) solid var(--tx);
  outline-offset: calc(-1 * var(--focus-ring));
}

.cell {
  display: flex;
  min-width: 0;
  height: var(--row-service);
}

.cell--name {
  flex-direction: column;
  justify-content: center;
  gap: var(--space-half);
}

.name-line,
.detail-line {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.name {
  min-width: 0;
  font-weight: var(--fw-semibold);
}

.detail-line {
  gap: var(--space-1);
}

.detail-line:not(:has(*)) {
  display: none;
}

.image {
  min-width: 0;
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

/* L'image est sur sa propre colonne en mode large, sous le nom en mode compact. */
.image--inline {
  display: none;
}

.cell--image {
  align-items: center;
}

.cell--state {
  flex-direction: column;
  justify-content: center;
  gap: var(--space-half);
}

.state-line {
  overflow: hidden;
  color: var(--tx2);
  font-size: var(--fs-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.state-line:empty {
  display: none;
}

.cell--ports {
  align-items: center;
  overflow: hidden;
}

.band {
  display: flex;
  align-items: center;
  grid-column: 1 / -1;
  gap: var(--space-2);
  min-width: 0;
  color: var(--warn);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

/* Mode compact : l'image passe sous le nom et la colonne « Image » disparaît (la zone de contenu fait moins de 960 px). */
@container svctable (max-width: 960px) {
  .table__head,
  .row {
    grid-template-columns: var(--svc-columns-compact);
  }

  .table__th--image,
  .cell--image {
    display: none;
  }

  .image--inline {
    display: inline-flex;
  }
}

@container svctable (max-width: 600px) {
  .table__head,
  .row {
    grid-template-columns: minmax(0, 1fr) minmax(120px, 170px);
  }

  .table__th--ports,
  .cell--ports {
    display: none;
  }

  /* Le repère d'origine ne tient pas sur la ligne : il est dans le nom accessible de la ligne et dans le détail. */
  .name-line :deep(.tag),
  .detail-line :deep(.tag) {
    display: none;
  }
}
</style>
