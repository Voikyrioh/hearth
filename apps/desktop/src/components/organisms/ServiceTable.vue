<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HMiddleText from "@/components/atoms/HMiddleText.vue";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import type { ServiceInfo } from "@/link";
import {
  imageLabel,
  loopBand,
  portLabel,
  rowLabel,
  stateLine,
  visiblePortCount,
} from "@/services/format";
import { anchoredScroll, offsetsOf, type RowHeights, windowOf } from "@/services/layout";
import OriginTags from "../molecules/OriginTags.vue";
import PortChips from "../molecules/PortChips.vue";
import ServiceStateBadge from "../molecules/ServiceStateBadge.vue";

// La liste des services (BR-SVC-001, 002, 012, 015, 019, 021) : une grille ARIA (en-têtes de colonnes,
// `aria-rowcount`), triée par nom par la liaison (l'ordre ne dépend jamais de l'état), VIRTUALISÉE (seules les
// lignes visibles et quelques voisines sont dans le DOM, sans pagination). La hauteur d'une ligne se DÉDUIT de
// la donnée (standard, ou plus haute pour « Redémarre en boucle »), elle n'est jamais mesurée : le défilement
// ne saute donc pas quand une ligne change d'état, et une ligne qui apparaît ou change au-dessus de la zone
// visible laisse la ligne repère où elle était. Une ligne est reconnue par sa clé stable, jamais par son
// index : le focus clavier la suit. Un seul arrêt de tabulation (la ligne active) ; flèches, Début, Fin et
// pages déplacent la ligne active. Tout texte est interpolé (jamais du HTML) ; un nom, une image de 255
// caractères sont coupés au milieu et complets au survol.
const props = defineProps<{ services: readonly ServiceInfo[] }>();

/** Lignes dessinées au-delà de la zone visible, de chaque côté. */
const OVERSCAN = 8;
/** Repli (tests sans mise en page) quand la zone n'a pas de hauteur mesurable. */
const FALLBACK_VIEWPORT = 600;
const FALLBACK_HEIGHTS: RowHeights = { row: 52, alert: 80 };

const now = useNow();
const scroller = ref<HTMLElement | null>(null);
const frame = ref<HTMLElement | null>(null);
const heights = ref<RowHeights>({ ...FALLBACK_HEIGHTS });
const range = ref({ start: 0, end: 30 });
const activeKey = ref<string | null>(null);
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
  const window = windowOf(offsets.value, el.scrollTop, viewport, OVERSCAN);
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

function onKeydown(event: KeyboardEvent) {
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

/**
 * Tous les ports au survol : la ligne n'en montre que trois, sur deux lignes au plus. Ceux que l'agent a coupés
 * au-delà de sa borne sont comptés à la suite, sans être listés (BR-SVC-021).
 */
function portsTitle(service: ServiceInfo): string | undefined {
  const omitted = service.portsOmitted;
  if (service.ports.length <= visiblePortCount(service.ports) && omitted === 0) return undefined;
  const labels = service.ports.map(portLabel);
  if (omitted > 0) {
    labels.push(
      t(labels.length > 0 ? "services.portsAlsoOmitted" : "services.portsOnlyOmitted", {
        n: omitted,
      }),
    );
  }
  return labels.join(", ");
}

const COLUMNS = [
  "services.colName",
  "services.colImage",
  "services.colState",
  "services.colPorts",
] as const;

defineExpose({ visible });
</script>

<template>
  <div class="table">
    <div
      class="table__grid"
      role="grid"
      :aria-label="t('services.listLabel')"
      :aria-rowcount="services.length + 1"
      :aria-colcount="COLUMNS.length"
      @keydown="onKeydown"
    >
      <div class="table__head" role="row" aria-rowindex="1">
        <div
          v-for="(key, column) in COLUMNS"
          :key="key"
          :class="['table__th', { 'table__th--image': column === 1 }]"
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
              :tabindex="index === activeIndex ? 0 : -1"
              @focus="activeKey = service.key"
            >
              <div class="cell cell--name" role="gridcell">
                <span class="name-line">
                  <HMiddleText class="name" :text="service.name" :focusable="false" />
                  <OriginTags :service="service" part="origin" />
                </span>
                <span class="detail-line">
                  <OriginTags :service="service" part="details" />
                  <HMiddleText
                    class="image image--inline"
                    :text="imageLabel(service)"
                    :focusable="false"
                  />
                </span>
              </div>
              <div class="cell cell--image" role="gridcell">
                <HMiddleText class="image" :text="imageLabel(service)" :focusable="false" />
              </div>
              <div class="cell cell--state" role="gridcell">
                <ServiceStateBadge :state="service.state" />
                <span class="state-line">{{ stateLine(service, now) }}</span>
              </div>
              <div class="cell cell--ports" role="gridcell" :title="portsTitle(service)">
                <PortChips :ports="service.ports" :omitted="service.portsOmitted" />
              </div>
              <div v-if="service.state === 'restart_loop'" class="band" role="gridcell" aria-colspan="4">
                <HIcon name="alert" size="sm" />
                <span>{{ loopBand(service) }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
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
</style>
