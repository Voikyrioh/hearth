<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import { formatDateTime } from "@/composables/format";
import { t } from "@/i18n";

// Information montrée UNE fois, à la connexion qui suit la fermeture des sessions du compte par un autre compte
// (HRT-133, BR-ACCT-019) : « Tes sessions ont été fermées par {identifiant} le {date}. » Jamais d'adresse ni de
// nom de poste. Fermable ; l'agent ne la répète pas.
defineProps<{ by: string; at: string }>();
defineEmits<{ dismiss: [] }>();
</script>

<template>
  <Callout tone="info" data-sessions-closed>
    <span data-sessions-closed-text>{{ t("sessionsClosed.banner", { username: by, date: formatDateTime(at) }) }}</span>
    <template #actions>
      <HButton
        variant="ghost"
        size="sm"
        :aria-label="t('sessionsClosed.dismiss')"
        data-sessions-closed-dismiss
        @click="$emit('dismiss')"
      >
        <HIcon name="close" size="sm" />
      </HButton>
    </template>
  </Callout>
</template>
