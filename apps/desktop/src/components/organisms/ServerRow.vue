<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";
import LinkStatePill from "@/components/molecules/LinkStatePill.vue";
import ServerAvatar from "@/components/molecules/ServerAvatar.vue";
import { t } from "@/i18n";
import type { ServerInfo } from "@/link";
import { markOf } from "@/security/mark";
import { useAgentUpdatesStore } from "@/stores/agentUpdates";
import { useLinkStore } from "@/stores/link";
import { useSecurityStore } from "@/stores/security";

// Une ligne du carnet de serveurs : nom, couleur, adresse, état du lien, et les actions du
// serveur. Chaque serveur a son état, indépendant des autres : ouvrir l'un ne ferme pas les
// autres connexions (BR-CONN-015).
const props = defineProps<{ server: ServerInfo }>();

defineEmits<{ edit: []; remove: []; disconnect: []; forget: [] }>();

const link = useLinkStore();
const state = computed(() => link.stateOf(props.server.id));
// Mode attaque, suspendu ou alerte de sécurité : une étiquette ÉCRITE après le nom (HRT-26).
const security = useSecurityStore();
const mark = computed(() => markOf(security.of(props.server.id)?.state));
const MARK_TAGS = {
  attack: "security.tagAttack",
  suspended: "security.tagSuspended",
  alert: "security.tagAlert",
} as const;
// Seul le serveur qui a une mise à jour de l'agent disponible le dit (BR-UPDATE-023).
const agents = useAgentUpdatesStore();
const updateAvailable = computed(() => agents.withUpdate.includes(props.server.id));
</script>

<template>
  <li class="row" :data-server-row="server.id">
    <ServerAvatar :name="server.name" :color="server.color" :state="state" :mark="mark" />
    <div class="row__id">
      <RouterLink
        class="row__name"
        :to="{ name: 'dashboard', params: { id: server.id } }"
        :aria-label="`${t('connect.open')} ${server.name}`"
      >
        {{ server.name }}
      </RouterLink>
      <span class="row__address">{{ server.address }}</span>
    </div>
    <HTag v-if="mark" :tone="mark === 'alert' ? 'warn' : 'accent'" :data-security-tag="mark">{{ t(MARK_TAGS[mark]) }}</HTag>
    <HTag v-if="server.remember" tone="neutral">{{ t("connect.rememberedBadge") }}</HTag>
    <HTag v-if="updateAvailable" tone="accent" data-update-available>{{ t("agentUpdate.tag") }}</HTag>
    <LinkStatePill :state="state" />
    <div class="row__actions">
      <HButton v-if="state === 'connected'" variant="secondary" size="sm" @click="$emit('disconnect')">
        {{ t("connect.disconnect") }}
      </HButton>
      <HButton v-if="server.remember" variant="secondary" size="sm" @click="$emit('forget')">
        {{ t("connect.forget") }}
      </HButton>
      <HButton variant="secondary" size="sm" @click="$emit('edit')">{{ t("connect.edit") }}</HButton>
      <HButton variant="danger" size="sm" @click="$emit('remove')">
        {{ t("connect.remove") }}
      </HButton>
    </div>
  </li>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-card);
  background: var(--card);
  list-style: none;
}

.row__id {
  /* FIX:01M4D4G1E0DGHFXWVPXVW04DF6 : la colonne du nom garde sa largeur de contenu, les étiquettes passent à la ligne au lieu de la recouvrir (revue UX C51) */
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  min-width: 0;
}

.row__name {
  font-weight: var(--fw-semibold);
}

.row__address {
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.row__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}
</style>
