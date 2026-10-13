<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";

// Bulle d'explication au survol ET au focus clavier (`role="tooltip"`), fermée par Échap.
// Le slot reçoit `describedby` à poser sur le contrôle pour que les lecteurs d'écran la lisent.
// Sans `text`, aucune bulle (l'enveloppe reste : le contrôle n'est jamais recréé quand
// l'explication apparaît ou disparaît, le focus clavier est conservé). `placement` aligne la bulle sur le
// début, le centre ou la fin du contrôle, `side` la met dessous ou dessus.
//
// HRT-134 : la bulle est rendue dans `body` (Teleport), jamais dans le conteneur du contrôle. Un tableau à
// défilement (`overflow`) ou un conteneur de requêtes (`container-type`) coupe tout ce qui le dépasse, même en
// `position: fixed`. Hors de lui, la bulle est placée à la main sous (ou sur) le contrôle, puis ramenée dans la
// fenêtre : ni coupée, ni cause d'une barre de défilement. Elle suit le contrôle si la page défile ou change de taille.
const props = withDefaults(
  defineProps<{ text?: string; placement?: "start" | "center" | "end"; side?: "bottom" | "top" }>(),
  { text: undefined, placement: "center", side: "bottom" },
);

/** Écart entre le contrôle et la bulle, et marge minimale avec le bord de la fenêtre (pixels CSS). */
const GAP = 8;

const id = useId();
const open = ref(false);
const trigger = ref<HTMLElement | null>(null);
const bubble = ref<HTMLElement | null>(null);

function place() {
  const anchor = trigger.value;
  const tip = bubble.value;
  if (!anchor || !tip) return;
  const a = anchor.getBoundingClientRect();
  const width = tip.offsetWidth;
  const height = tip.offsetHeight;
  const viewW = document.documentElement.clientWidth;
  const viewH = document.documentElement.clientHeight;
  let x = a.left + a.width / 2 - width / 2;
  if (props.placement === "start") x = a.left;
  if (props.placement === "end") x = a.right - width;
  x = Math.max(GAP, Math.min(x, viewW - width - GAP));
  // Dessous par défaut ; s'il n'y a pas la place, de l'autre côté (sans jamais sortir par le haut).
  const below = a.bottom + GAP;
  const above = a.top - GAP - height;
  let y = props.side === "top" ? above : below;
  if (props.side === "top" && above < GAP) y = below;
  if (props.side === "bottom" && below + height > viewH - GAP && above >= GAP) y = above;
  tip.style.setProperty("--tip-x", `${Math.round(x)}px`);
  tip.style.setProperty("--tip-y", `${Math.round(y)}px`);
}

function onViewportChange() {
  place();
}

/** Position du contrôle au dernier passage : le déclencheur peut bouger sans que rien défile ni se redimensionne. */
let last = "";
let frame = 0;
let watcher: IntersectionObserver | undefined;

function follow() {
  const anchor = trigger.value;
  if (anchor) {
    const r = anchor.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) {
      // Plus affiché (retiré de la page, masqué) : rien à expliquer.
      open.value = false;
      return;
    }
    const now = `${r.left}|${r.top}|${r.width}|${r.height}`;
    if (now !== last) {
      last = now;
      place();
    }
  }
  frame = requestAnimationFrame(follow);
}

function stop() {
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("scroll", onViewportChange, true);
  cancelAnimationFrame(frame);
  watcher?.disconnect();
  watcher = undefined;
}

watch(open, async (isOpen) => {
  if (!isOpen) {
    stop();
    // `?.` : sans `popover` (anciens moteurs, jsdom), `v-show` suffit à cacher la bulle.
    bubble.value?.hidePopover?.();
    return;
  }
  await nextTick();
  if (!open.value) return;
  bubble.value?.showPopover?.();
  place();
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("scroll", onViewportChange, true);
  last = "";
  if (typeof requestAnimationFrame === "function") frame = requestAnimationFrame(follow);
  // Le contrôle défile hors du cadre de son conteneur (en-tête qui le cache) : la bulle ne flotte pas sans lui.
  if (typeof IntersectionObserver === "function" && trigger.value) {
    watcher = new IntersectionObserver((entries) => {
      if (entries.some((entry) => !entry.isIntersecting)) open.value = false;
    });
    watcher.observe(trigger.value);
  }
});

onBeforeUnmount(stop);
</script>

<template>
  <span
    ref="trigger"
    class="tip"
    @mouseenter="open = true"
    @mouseleave="open = false"
    @focusin="open = true"
    @focusout="open = false"
    @keydown.esc="open = false"
  >
    <slot :describedby="text ? id : undefined" />
    <Teleport to="body">
      <span
        v-if="text"
        v-show="open"
        :id="id"
        ref="bubble"
        class="tip__bubble"
        popover="manual"
        role="tooltip"
        >{{ text }}</span
      >
    </Teleport>
  </span>
</template>

<style scoped>
.tip {
  display: inline-flex;
}

/* Hors du conteneur du contrôle : position fixe dans la fenêtre, coordonnées posées par `place()`. */
.tip__bubble {
  position: fixed;
  /* `popover` pose des marges et `inset: 0` par défaut : on repart de zéro pour que le placement soit le nôtre. */
  inset: auto;
  margin: 0;
  overflow: visible;
  top: var(--tip-y, 0);
  left: var(--tip-x, 0);
  z-index: var(--z-tooltip);
  width: max-content;
  max-width: min(var(--tooltip-max), 100vw);
  box-sizing: border-box;
  padding: var(--space-2) var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
  font-size: var(--fs-small);
  font-weight: var(--fw-regular);
  white-space: normal;
  pointer-events: none;
}
</style>
