<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import { useCopyText } from "@/composables/useCopyText";
import { t } from "@/i18n";
import type { GamePort } from "@/link";
import { portRows, portsChanged, portsListText } from "@/services/game";

// La carte « Ports à ouvrir » du détail d'un serveur de jeu (HRT-104, BR-JEU-017 à BR-JEU-019) : chaque port publié, son
// protocole et son rôle, visible de tous les profils. Elle coexiste avec la carte « Ports » du détail (port du serveur vers
// port du service) : l'une sert à relier un port, l'autre à l'ouvrir sur la box. Elle ne promet AUCUNE joignabilité :
// Hearth n'ouvre rien lui-même et ne peut pas vérifier que les ports sont joignables depuis Internet. Quand un numéro
// diffère de celui de la fiche, un rappel permanent le dit.
const props = defineProps<{ ports: readonly GamePort[] }>();

const copier = useCopyText(() => t("game.ports.listCopied"));
const rows = () => portRows(props.ports);
</script>

<template>
  <DashCard :title="t('game.ports.title')" data-game-ports>
    <template v-if="rows().length > 0" #aside>
      <HButton
        variant="secondary"
        size="sm"
        data-game-ports-copy
        @click="copier.copy(portsListText(ports))"
      >
        <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
        {{ t("game.ports.copyList") }}
      </HButton>
    </template>

    <table v-if="rows().length > 0" class="ports" :aria-label="t('game.ports.table')" data-game-ports-table>
      <thead>
        <tr>
          <th scope="col">{{ t("game.ports.colPort") }}</th>
          <th scope="col">{{ t("game.ports.colProtocol") }}</th>
          <th scope="col">{{ t("game.ports.colRole") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows()" :key="row.key" data-game-port-row>
          <td class="ports__num">{{ row.port }}</td>
          <td>{{ row.protocol }}</td>
          <td>{{ row.role }}</td>
        </tr>
      </tbody>
    </table>
    <p v-else class="ports__note" data-game-ports-none>{{ t("game.ports.none") }}</p>

    <Callout v-if="copier.state.value === 'failed'" tone="warn" inline data-game-ports-failed>
      {{ t("game.ports.listFailed") }}
    </Callout>
    <template v-if="rows().length > 0">
      <p class="ports__note" data-game-ports-open>{{ t("game.ports.open") }}</p>
      <p class="ports__note" data-game-ports-unverified>
        <HIcon name="info" size="sm" />
        {{ t("game.ports.unverified") }}
      </p>
      <Callout v-if="portsChanged(ports)" tone="warn" inline data-game-ports-changed>
        {{ t("game.ports.changed") }}
      </Callout>
    </template>
  </DashCard>
</template>

<style scoped>
.ports {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.ports th,
.ports td {
  height: var(--target-size);
  padding: 0 var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
}

.ports th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
}

.ports__num {
  font-family: var(--font-mono);
  font-weight: var(--fw-semibold);
}

.ports__note {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  overflow-wrap: anywhere;
}
</style>
