<script setup lang="ts">
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { Pending } from "@/composables/useServiceConsole";
import { t } from "@/i18n";

// La confirmation d'une commande de console (HRT-269, BR-CONSOLE-004, 005, maquette ConsoleSensible) : pour une commande
// SENSIBLE, le titre et la phrase de la fiche (« Donner les droits d'opérateur à Bram ? »), la commande telle qu'elle
// partira, le mot de passe demandé À CHAQUE commande de ce genre (la phrase qui le dit vient après le champ) ; pour une commande libre
// que l'état de confiance soumet au mot de passe (alerte, attaque), la même fenêtre sans le titre propre au verbe. Rien
// ne part avant « Envoyer la commande » ; « Annuler » ne laisse rien partir.
defineProps<{
  serverId: string;
  pending: Pending | null;
  perform: (adminPassword: string | null) => Promise<ActReport>;
  refusalText: () => string | undefined;
}>();
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <AdminActDialog
    :open="pending !== null"
    :server-id="serverId"
    :kind="pending?.class === 2 ? 'console_command_sensitive' : 'console_command_free'"
    :title="pending && pending.class === 2 ? pending.title : t('console.confirm.freeTitle')"
    :submit-label="t('console.confirm.send')"
    :cancel-label="t('console.confirm.cancel')"
    :perform="perform"
    :refusal-text="refusalText"
    @close="emit('close')"
  >
    <div v-if="pending" class="cs" data-console-confirm>
      <p class="cs__command" data-console-confirm-command><span aria-hidden="true">&gt;</span> {{ pending.command }}</p>
      <p v-if="pending.sentence" class="cs__sentence" data-console-confirm-sentence>{{ pending.sentence }}</p>
    </div>
    <template #after>
      <p v-if="pending" class="cs__note" data-console-confirm-note>
        {{ pending.class === 2 ? t("console.confirm.eachTime") : t("console.confirm.trust") }}
      </p>
    </template>
  </AdminActDialog>
</template>

<style scoped>
.cs {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.cs__command {
  margin: 0;
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--bg);
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}

.cs__sentence,
.cs__note {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
