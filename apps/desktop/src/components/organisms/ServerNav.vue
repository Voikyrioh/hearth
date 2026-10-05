<script setup lang="ts">
import { computed, ref } from "vue";
import { RouterLink } from "vue-router";
import { arrowNav } from "@/composables/arrowNav";
import { type MessageKey, t } from "@/i18n";
import type { ServerInfo } from "@/link";

// Navigation du serveur : nom, adresse en chasse fixe, entrées de vue. « Comptes » et
// « Journal d'activité » n'existent pas pour le rôle Lecture seule (BR-ACCT-013).
const props = defineProps<{ server: ServerInfo }>();

interface Entry {
  route: "dashboard" | "accounts" | "audit";
  label: MessageKey;
  adminOnly: boolean;
}

const ENTRIES: Entry[] = [
  { route: "dashboard", label: "nav.dashboard", adminOnly: false },
  { route: "accounts", label: "nav.accounts", adminOnly: true },
  { route: "audit", label: "nav.audit", adminOnly: true },
];

const entries = computed(() =>
  ENTRIES.filter((entry) => !entry.adminOnly || props.server.role === "admin"),
);
const nav = ref<HTMLElement | null>(null);
</script>

<template>
  <aside class="side">
    <div class="side__head">
      <strong class="side__name">{{ server.name }}</strong>
      <small class="side__address">{{ server.address }}</small>
    </div>
    <nav
      ref="nav"
      class="nav"
      :aria-label="t('nav.label')"
      @keydown="arrowNav($event, nav, '[data-nav-item]')"
    >
      <RouterLink
        v-for="entry in entries"
        :key="entry.route"
        class="nav__item"
        :to="{ name: entry.route, params: { id: server.id } }"
        data-nav-item
      >
        {{ t(entry.label) }}
      </RouterLink>
    </nav>
  </aside>
</template>

<style scoped>
.side {
  display: flex;
  flex: none;
  flex-direction: column;
  gap: var(--space-4);
  width: var(--nav-width);
  padding: var(--space-5) var(--space-3);
  border-right: var(--border-width) solid var(--bd);
  background: var(--side);
}

.side__head {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.side__name {
  overflow: hidden;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.side__address {
  overflow: hidden;
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  text-overflow: ellipsis;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: var(--space-half);
}

.nav__item {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  color: var(--tx2);
}

.nav__item::before {
  content: "";
  width: var(--nav-puck);
  height: var(--nav-puck);
  border-radius: var(--radius-puck);
  background: currentColor;
  opacity: var(--opacity-muted);
}

.nav__item:hover {
  color: var(--tx);
}

.nav__item.router-link-active {
  background: var(--card);
  color: var(--tx);
}

.nav__item.router-link-active::before {
  background: var(--ac);
  opacity: 1;
}
</style>
