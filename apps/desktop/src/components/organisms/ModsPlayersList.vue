<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HTag from "@/components/atoms/HTag.vue";
import { useCopyText } from "@/composables/useCopyText";
import { t } from "@/i18n";
import type { ModsView } from "@/link";
import { loaderName, modName } from "@/services/mods";
import { audienceOf, audienceReason, copyText, entryOf } from "@/services/modsCopy";

// « Ce que vos joueurs doivent installer » (HRT-264, BR-MOD-029, maquette Joueurs) : chaque mod ACTIVÉ, « Aussi chez les
// joueurs » ou « Serveur seulement » d'après ce que le mod dit de lui-même (jamais « Obligatoire » : Hearth ne le sait
// pas), l'adresse du serveur en tête quand elle est connue, et un seul bouton, « Copier la liste ». Aucun fichier à
// enregistrer, aucun fichier pour le lanceur (décision de Voiky du 2026-11-16).
const props = defineProps<{
  view: ModsView;
  /** L'adresse que les joueurs utilisent (domaine ou IP et port), si l'agent la connaît. */
  address: string | null;
}>();
const emit = defineEmits<{ back: [] }>();

const copier = useCopyText(() => t("mods.players.copied"));
const mods = computed(() =>
  props.view.mods.filter((mod) => mod.place === "active" && mod.enabled && mod.identity !== null),
);
const rows = computed(() =>
  mods.value.map((mod) => ({
    key: `${mod.rank}-${mod.file ?? ""}`,
    name: modName(mod),
    version: mod.identity?.version ?? "",
    audience: audienceOf(mod.identity?.environment ?? "unknown"),
    reason: audienceReason(mod.identity?.environment ?? "unknown"),
  })),
);
const text = computed(() =>
  copyText({
    address: props.address,
    minecraft: props.view.game.minecraft,
    entries: mods.value.flatMap((mod) => (mod.identity ? [entryOf(mod.identity)] : [])),
  }),
);
</script>

<template>
  <section class="mpl" data-mods-players-list>
    <HButton variant="ghost" size="sm" class="mpl__back" data-mods-players-back @click="emit('back')">
      <HIcon name="arrow-left" size="sm" />
      {{ t("mods.deposit.back") }}
    </HButton>
    <h3 class="mpl__title">{{ t("mods.players.title") }}</h3>
    <p class="mpl__lead" data-mods-players-lead>
      {{ t("mods.players.lead", { minecraft: view.game.minecraft ?? "?", loader: loaderName(view.game.loader) }) }}
    </p>
    <div class="mpl__bar">
      <HButton variant="primary" data-mods-players-copy @click="copier.copy(text)">
        <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
        {{ t(copier.state.value === "copied" ? "mods.players.copiedButton" : "mods.players.copy") }}
      </HButton>
      <span class="mpl__hint">{{ t("mods.players.copyHint") }}</span>
    </div>
    <p v-if="copier.state.value === 'failed'" class="mpl__hint" role="alert" data-mods-players-failed>
      {{ t("mods.players.copyFailed") }}
    </p>
    <p v-if="address" class="mpl__address" data-mods-players-address>
      {{ t("mods.players.address", { address }) }}
    </p>
    <div class="mpl__wrap">
      <table class="mpl__table" data-mods-players-table>
        <thead>
          <tr>
            <th scope="col">{{ t("mods.table.mod") }}</th>
            <th scope="col">{{ t("mods.table.version") }}</th>
            <th scope="col">{{ t("mods.players.where") }}</th>
            <th scope="col">{{ t("mods.players.why") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.key" data-mods-players-row :data-audience="row.audience">
            <td class="mpl__name">{{ row.name }}</td>
            <td class="mpl__mono">{{ row.version }}</td>
            <td>
              <HTag :tone="row.audience === 'players' ? 'accent' : 'neutral'">
                {{ t(row.audience === "players" ? "mods.players.both" : "mods.players.serverOnly") }}
              </HTag>
            </td>
            <td class="mpl__why">{{ row.reason }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <p class="mpl__hint" data-mods-players-note>{{ t("mods.players.note") }}</p>
    <section class="mpl__soon">
      <h4 class="mpl__subtitle">{{ t("mods.players.soonTitle") }}</h4>
      <p class="mpl__hint">{{ t("mods.players.soon") }}</p>
    </section>
  </section>
</template>

<style scoped>
.mpl {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.mpl__back {
  align-self: flex-start;
}

.mpl__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.mpl__lead,
.mpl__hint,
.mpl__address {
  margin: 0;
  color: var(--tx-secondary);
}

.mpl__hint {
  font-size: var(--fs-small);
}

.mpl__address {
  font-family: var(--font-mono);
}

.mpl__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.mpl__wrap {
  overflow-x: auto;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.mpl__table {
  width: 100%;
  border-collapse: collapse;
}

.mpl__table th,
.mpl__table td {
  padding: var(--space-2) var(--space-3);
  text-align: left;
}

.mpl__table th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
}

.mpl__table td {
  border-top: var(--border-width) solid var(--bd);
}

.mpl__name {
  font-weight: var(--fw-semibold);
}

.mpl__mono {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.mpl__why {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.mpl__soon {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) dashed var(--bd);
  border-radius: var(--radius-card);
}

.mpl__subtitle {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}
</style>
