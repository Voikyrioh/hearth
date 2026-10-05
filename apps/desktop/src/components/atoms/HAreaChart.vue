<script setup lang="ts">
import { computed, useId } from "vue";
import type { Point } from "@/dashboard/series";

// Courbe pleine (SVG pur) : trait 2 px, remplissage dégradé, point d'extrémité plein, pas de
// grille. Les pas sans mesure (`v` nul) sont des TROUS (le trait s'interrompt), jamais un zéro.
// Plusieurs séries se superposent. `max` nul : l'échelle suit la plus grande valeur affichée.
export interface ChartSeries {
  points: readonly Point[];
  tone: "ac" | "cool";
}

const props = defineProps<{
  series: readonly ChartSeries[];
  max: number | null;
  /** Échelle minimale quand `max` est nul : une courbe de température ne plafonne pas à sa valeur. */
  atLeast?: number;
  label: string;
}>();

const WIDTH = 300;
const HEIGHT = 80;
const PAD = 4;
const prefix = useId();

const ceiling = computed(() => {
  if (props.max !== null) return props.max;
  let top = 0;
  for (const serie of props.series) {
    for (const point of serie.points) if (point.v !== null && point.v > top) top = point.v;
  }
  return Math.max(top, props.atLeast ?? 1);
});

interface Drawn {
  tone: ChartSeries["tone"];
  line: string;
  area: string;
  dot: { x: number; y: number } | null;
}

const drawn = computed<Drawn[]>(() =>
  props.series.map((serie) => {
    const count = serie.points.length;
    const stepX = count > 1 ? (WIDTH - PAD * 2) / (count - 1) : 0;
    const top = ceiling.value;
    const x = (index: number) => PAD + index * stepX;
    const y = (value: number) =>
      HEIGHT - PAD - (Math.min(Math.max(value, 0), top) / top) * (HEIGHT - PAD * 2);
    let line = "";
    let area = "";
    let run: { from: number; to: number } | null = null;
    let dot: Drawn["dot"] = null;
    const close = () => {
      if (run) {
        area += `L ${x(run.to).toFixed(1)} ${HEIGHT - PAD} L ${x(run.from).toFixed(1)} ${HEIGHT - PAD} Z `;
      }
      run = null;
    };
    serie.points.forEach((point, index) => {
      if (point.v === null) {
        close();
        return;
      }
      const at = `${x(index).toFixed(1)} ${y(point.v).toFixed(1)}`;
      if (run) {
        line += `L ${at} `;
        area += `L ${at} `;
        run.to = index;
      } else {
        line += `M ${at} `;
        area += `M ${at} `;
        run = { from: index, to: index };
      }
      dot = { x: x(index), y: y(point.v) };
    });
    close();
    return { tone: serie.tone, line: line.trim(), area: area.trim(), dot };
  }),
);
</script>

<template>
  <svg class="chart" :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" role="img" :aria-label="label">
    <defs>
      <linearGradient
        v-for="(serie, index) in drawn"
        :id="`${prefix}-${index}`"
        :key="index"
        x1="0"
        y1="0"
        x2="0"
        y2="1"
      >
        <stop offset="0" :class="['chart__fill', `chart__fill--${serie.tone}`]" />
        <stop offset="1" class="chart__clear" />
      </linearGradient>
    </defs>
    <template v-for="(serie, index) in drawn" :key="index">
      <path v-if="serie.area" :d="serie.area" :fill="`url(#${prefix}-${index})`" stroke="none" />
      <path
        v-if="serie.line"
        :class="['chart__line', `chart__line--${serie.tone}`]"
        :d="serie.line"
      />
      <circle
        v-if="serie.dot"
        :class="`chart__dot--${serie.tone}`"
        :cx="serie.dot.x"
        :cy="serie.dot.y"
        r="3"
      />
    </template>
  </svg>
</template>

<style scoped>
.chart {
  display: block;
  width: 100%;
  height: auto;
  min-height: var(--chart-min-height);
}

.chart__fill {
  stop-opacity: var(--chart-fill-opacity);
}

.chart__fill--ac {
  stop-color: var(--ac);
}

.chart__fill--cool {
  stop-color: var(--cool);
}

.chart__clear {
  stop-color: transparent;
  stop-opacity: 0;
}

.chart__line {
  fill: none;
  stroke-width: 2;
  stroke-linejoin: round;
  stroke-linecap: round;
}

.chart__line--ac {
  stroke: var(--ac);
}

.chart__line--cool {
  stroke: var(--cool);
}

.chart__dot--ac {
  fill: var(--ac);
}

.chart__dot--cool {
  fill: var(--cool);
}
</style>
