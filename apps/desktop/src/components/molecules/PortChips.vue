<script setup lang="ts">
import { computed } from "vue";
import { t } from "@/i18n";
import type { ServicePort } from "@/link";
import { allPortsText, PORTS_WIDTH, portLabel, visiblePortCount } from "@/services/format";

// Les ports exposés d'un service : au plus 3 puces, et seulement celles qui tiennent sur deux lignes « {port du serveur} vers {port du service} ({protocole}) »,
// puis « +N » (BR-SVC-002). Aucun port : « Aucun port ». Un service qui utilise directement le réseau du
// serveur n'a pas de port individuel (BR-SVC-014) : rien n'est inventé, et la cellule le DIT (jamais « Aucun
// port », faux et rassurant : ce service écoute sur toutes les interfaces du serveur). Les ports que l'agent a
// coupés au-delà de sa borne (`omitted`, BR-SVC-021) comptent dans le « +N » sans être listés. « +N » est un
// bouton (clic : la fenêtre de tous les ports) : pas un arrêt de tabulation, la ligne n'en a qu'un ; au clavier
// c'est Entrée sur la ligne.
const props = withDefaults(
  defineProps<{
    ports: readonly ServicePort[];
    omitted?: number;
    hostNetwork?: boolean;
    /** Largeur de la colonne « Ports » (270 en large, 220 en compact). */
    width?: number;
  }>(),
  { omitted: 0, hostNetwork: false, width: PORTS_WIDTH.wide },
);

const emit = defineEmits<{ more: [] }>();

const empty = computed(() => props.ports.length === 0 && props.omitted === 0);
const shown = computed(() =>
  props.ports.slice(0, visiblePortCount(props.ports, props.width, props.omitted)),
);
const more = computed(() => Math.max(0, props.ports.length - shown.value.length) + props.omitted);
</script>

<template>
  <span v-if="empty && hostNetwork" class="chips chips--host" data-host-network>{{
    t("services.hostNetwork")
  }}</span>
  <span v-else-if="empty" class="chips chips--none">{{ t("services.noPort") }}</span>
  <span v-else class="chips">
    <span v-for="(port, index) in shown" :key="index" class="chip">{{ portLabel(port) }}</span>
    <button
      v-if="more > 0"
      type="button"
      class="chip chip--more"
      tabindex="-1"
      data-more-ports
      :title="allPortsText(ports, omitted)"
      :aria-label="t('services.portsOpen', { n: more })"
      @click.stop="emit('more')"
    >
      {{ t("services.portMore", { n: more }) }}
    </button>
  </span>
</template>

<style scoped>
.chips {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
}

.chips--none {
  color: var(--tx3);
}

.chips--host {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.chip {
  padding: var(--space-half) var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--bg);
  color: var(--tx);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
}

.chip--more {
  border: var(--border-width) solid var(--bd);
  background: transparent;
  color: var(--tx2);
  cursor: pointer;
}
</style>
