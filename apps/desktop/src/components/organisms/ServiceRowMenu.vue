<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";
import { type Controls, type Gesture, ICONS, LABEL_KEYS } from "@/services/lifecycle";

// Le menu d'actions d'une ligne de la liste (HRT-65, BR-LIFE-003) : les mêmes gestes que le détail, pour les comptes
// qui ont le droit, aux seules entrées que l'état propose. Aucune entrée proposée : le bouton n'existe pas (le parent
// ne monte pas ce composant). Le bloquer (lien, moteur, verrou) le laisse visible, `aria-disabled`, focusable, avec
// l'infobulle de la raison, et il n'ouvre rien. Choisir une entrée ouvre le même parcours que les boutons du détail.
// Le menu est rendu dans `body` (la liste coupe ce qui la dépasse) et placé à la main sous le bouton, comme les
// infobulles. Flèches, Début, Fin, Échap ; Tab le ferme.
const props = defineProps<{ controls: Controls; name: string }>();
const emit = defineEmits<{ pick: [gesture: Gesture] }>();

/** Écart entre le bouton et le menu, et marge minimale avec le bord de la fenêtre (pixels CSS). */
const GAP = 4;

const menuId = useId();
const open = ref(false);
const anchor = ref<HTMLElement | null>(null);
const menu = ref<HTMLElement | null>(null);

function button(): HTMLElement | null {
  return anchor.value?.querySelector("button") ?? null;
}

function place() {
  const origin = button();
  const popup = menu.value;
  if (!origin || !popup) return;
  const a = origin.getBoundingClientRect();
  const width = popup.offsetWidth;
  const height = popup.offsetHeight;
  const viewW = document.documentElement.clientWidth;
  const viewH = document.documentElement.clientHeight;
  const x = Math.max(GAP, Math.min(a.right - width, viewW - width - GAP));
  const below = a.bottom + GAP;
  const above = a.top - GAP - height;
  const y = below + height > viewH - GAP && above >= GAP ? above : below;
  popup.style.setProperty("--menu-x", `${Math.round(x)}px`);
  popup.style.setProperty("--menu-y", `${Math.round(y)}px`);
}

function items(): HTMLElement[] {
  return [...(menu.value?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
}

function onOutside(event: Event) {
  const target = event.target;
  if (!(target instanceof Node)) return;
  if (menu.value?.contains(target) || anchor.value?.contains(target)) return;
  close(false);
}

function listen(on: boolean) {
  const method = on ? "addEventListener" : "removeEventListener";
  document[method]("pointerdown", onOutside, true);
  window[method]("resize", place);
  window[method]("scroll", place, true);
}

async function show() {
  open.value = true;
  listen(true);
  await nextTick();
  place();
  items()[0]?.focus();
}

/** Ferme le menu ; `restore` rend le focus au bouton (clavier), pas quand on clique ailleurs. */
function close(restore: boolean) {
  if (!open.value) return;
  open.value = false;
  listen(false);
  if (restore) button()?.focus();
}

function toggle() {
  if (open.value) close(true);
  else void show();
}

function pick(gesture: Gesture) {
  close(true);
  emit("pick", gesture);
}

function onMenuKeydown(event: KeyboardEvent) {
  const list = items();
  const index = list.indexOf(document.activeElement as HTMLElement);
  switch (event.key) {
    case "ArrowDown":
      list[(index + 1) % list.length]?.focus();
      break;
    case "ArrowUp":
      list[(index - 1 + list.length) % list.length]?.focus();
      break;
    case "Home":
      list[0]?.focus();
      break;
    case "End":
      list.at(-1)?.focus();
      break;
    case "Escape":
      close(true);
      break;
    case "Tab":
      close(false);
      return;
    default:
      return;
  }
  event.preventDefault();
}

onBeforeUnmount(() => listen(false));
</script>

<template>
  <!-- La ligne s'ouvre au clic et à Entrée : rien de ce qui se passe ici ne doit la rejoindre -->
  <span ref="anchor" class="rowmenu" data-row-menu @click.stop @keydown.stop>
    <HButton
      variant="ghost"
      size="sm"
      needs-link
      :disabled="controls.disabled"
      :hint="controls.reason ?? undefined"
      tabindex="-1"
      aria-haspopup="menu"
      :aria-expanded="open ? 'true' : 'false'"
      :aria-controls="open ? menuId : undefined"
      :aria-label="t('lifecycle.menuLabel', { name })"
      data-row-menu-button
      @click="toggle"
    >
      <HIcon name="more" size="sm" />
    </HButton>
    <Teleport to="body">
      <ul
        v-if="open"
        :id="menuId"
        ref="menu"
        class="rowmenu__list"
        role="menu"
        :aria-label="t('lifecycle.menuLabel', { name })"
        data-row-menu-list
        @keydown="onMenuKeydown"
      >
        <li v-for="gesture in controls.gestures" :key="gesture" role="none">
          <button
            type="button"
            role="menuitem"
            class="rowmenu__item"
            :data-row-menu-item="gesture"
            @click="pick(gesture)"
          >
            <HIcon
              :name="ICONS[gesture]"
              size="sm"
            />
            {{ t(LABEL_KEYS[gesture]) }}
          </button>
        </li>
      </ul>
    </Teleport>
  </span>
</template>

<style scoped>
.rowmenu {
  display: inline-flex;
}

.rowmenu__list {
  position: fixed;
  top: var(--menu-y, 0);
  left: var(--menu-x, 0);
  z-index: var(--z-menu);
  box-sizing: border-box;
  min-width: var(--menu-width);
  margin: 0;
  padding: var(--space-1);
  list-style: none;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.rowmenu__item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  box-sizing: border-box;
  width: 100%;
  min-height: var(--target-size);
  padding: 0 var(--space-3);
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--tx);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.rowmenu__item:hover,
.rowmenu__item:focus-visible {
  background: var(--card-2);
}

.rowmenu__item:focus-visible {
  outline: var(--focus-ring) solid var(--tx);
  outline-offset: calc(-1 * var(--focus-ring));
}
</style>
