<script setup lang="ts">
import { ref } from "vue";
import { RouterLink, useRoute } from "vue-router";
import HIcon from "@/components/atoms/HIcon.vue";
import HLogo from "@/components/atoms/HLogo.vue";
import ServerAvatar from "@/components/molecules/ServerAvatar.vue";
import { arrowNav } from "@/composables/arrowNav";
import { t } from "@/i18n";
import { markOf } from "@/security/mark";
import { useLinkStore } from "@/stores/link";
import { useSecurityStore } from "@/stores/security";
import { useServersStore } from "@/stores/servers";

// Barre des serveurs : logo, un avatar par serveur (sélection = ouvrir son tableau de bord),
// bouton « + », réglages. Flèches haut/bas : on passe d'un contrôle au suivant.
const servers = useServersStore();
const link = useLinkStore();
const security = useSecurityStore();
const route = useRoute();
const rail = ref<HTMLElement | null>(null);

function isActive(id: string): boolean {
  return route.params.id === id;
}
</script>

<template>
  <nav
    ref="rail"
    class="rail"
    :aria-label="t('rail.label')"
    @keydown="arrowNav($event, rail, '[data-rail-item]')"
  >
    <RouterLink class="rail__link" to="/" :aria-label="t('rail.home')" data-rail-item>
      <HLogo size="sm" decorative />
    </RouterLink>
    <RouterLink
      v-for="server in servers.servers"
      :key="server.id"
      class="rail__link"
      :to="{ name: 'dashboard', params: { id: server.id } }"
      :aria-current="isActive(server.id) ? 'true' : undefined"
      :data-server="server.id"
      data-rail-item
    >
      <ServerAvatar
        :name="server.name"
        :color="server.color"
        :state="link.stateOf(server.id)"
        :active="isActive(server.id)"
        :mark="markOf(security.of(server.id)?.state)"
      />
    </RouterLink>
    <RouterLink
      class="rail__link rail__link--tool"
      :to="{ name: 'add-server' }"
      :aria-label="t('rail.addServer')"
      data-rail-item
      data-rail-add
    >
      <HIcon name="plus" />
    </RouterLink>
    <RouterLink
      v-if="servers.servers.length > 0"
      class="rail__link rail__link--tool"
      :to="{ name: 'servers' }"
      :aria-label="t('rail.servers')"
      data-rail-item
      data-rail-servers
    >
      <HIcon name="server" />
    </RouterLink>
    <span class="rail__spacer" />
    <RouterLink
      class="rail__link rail__link--tool"
      to="/settings"
      :aria-label="t('rail.settings')"
      data-rail-item
    >
      <HIcon name="settings" />
    </RouterLink>
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  align-items: center;
  flex: none;
  gap: var(--space-3);
  width: var(--rail-width);
  padding: var(--space-4) 0;
  border-right: var(--border-width) solid var(--bd);
  background: var(--side);
}

.rail__link {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: var(--target-size);
  min-height: var(--target-size);
  border-radius: var(--radius-control);
}

.rail__link--tool {
  color: var(--tx2);
}

.rail__link--tool:hover,
.rail__link--tool.router-link-active {
  background: var(--card-2);
  color: var(--tx);
}

.rail__spacer {
  flex: 1;
}
</style>
