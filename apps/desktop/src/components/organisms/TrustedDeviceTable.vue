<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import { formatAgo } from "@/composables/formatAgo";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import type { TrustedDevice } from "@/link";
import type { DevicesStatus } from "@/stores/devices";

// « Tes postes de confiance » : la liste des postes que le serveur reconnaît pour ton compte (nom,
// dernière utilisation) et ce qu'on peut en faire. Ce poste est marqué et ne se retire pas depuis
// lui-même (« Retirer » grisé, avec son explication). Sans clé inscrite pour CE PC (aucun poste n'est
// « courant »), aucun retrait n'est possible : la preuve de la clé manque ; on le dit en tête de
// carte, et chaque « Retirer » en donne la raison. Les états (chargement, vide, 8 sur 8, erreur,
// agent trop ancien) ont chacun leur rendu. La liste dérive du gabarit de serveur : hors « Connecté »
// c'est `StaleSurface` qui la désature, et `needs-link` qui désactive « Retirer » (BR-RESIL-007, 008).
// Aucune clé, aucune empreinte de clé, aucun défi n'existe côté interface : seulement des noms, des
// dates, une adresse et des booléens.
const { isConnected } = useCurrentServer();

const props = defineProps<{
  status: DevicesStatus;
  devices: readonly TrustedDevice[];
  max: number;
  busy?: boolean;
}>();
const emit = defineEmits<{ remove: [device: TrustedDevice]; retry: [] }>();

const now = useNow();

/** Ce poste d'abord, puis la dernière utilisation la plus récente. */
const sorted = computed(() =>
  [...props.devices].sort((a, b) => {
    if (a.current !== b.current) return a.current ? -1 : 1;
    return Date.parse(b.lastProvedAt) - Date.parse(a.lastProvedAt);
  }),
);
const enrolled = computed(() => props.devices.some((device) => device.current));
const atLimit = computed(() => props.devices.length >= props.max);
const showLimitHelp = computed(() => atLimit.value && !enrolled.value);
const counter = computed(() => t("devices.counter", { n: props.devices.length, max: props.max }));

function lastUse(device: TrustedDevice): string {
  const at = Date.parse(device.lastProvedAt);
  return Number.isNaN(at) ? t("devices.never") : formatAgo(at, now.value);
}

function hintFor(device: TrustedDevice): string | undefined {
  if (device.current) return t("devices.currentHint");
  if (!enrolled.value) return t("devices.notEnrolledHint");
  return undefined;
}

const named = (device: TrustedDevice) => `${t("devices.remove")} ${device.name}`;
</script>

<template>
  <section class="card" :aria-busy="status === 'loading' ? 'true' : undefined">
    <header class="card__head">
      <h2 class="card__title">{{ t("devices.title") }}</h2>
      <span
        v-if="status !== 'loading' && status !== 'unsupported' && devices.length > 0"
        class="card__counter"
      >
        <HIcon v-if="atLimit" name="info" size="sm" />
        {{ counter }}
      </span>
    </header>

    <div v-if="status === 'loading' && devices.length === 0" class="card__wait">
      <ul class="skeleton" aria-hidden="true">
        <li class="skeleton__row" />
        <li class="skeleton__row" />
        <li class="skeleton__row" />
      </ul>
      <p class="card__loading" role="status">
        <HSpinner />
        <span>{{ t("devices.loading") }}</span>
      </p>
    </div>

    <p v-else-if="status === 'unsupported'" class="card__note">
      <HIcon name="info" size="sm" />
      <span>{{ t("devices.unsupported") }}</span>
    </p>

    <p
      v-else-if="status === 'error' && devices.length === 0 && !isConnected"
      class="card__note"
      data-not-loaded-yet
    >
      <HIcon name="info" size="sm" />
      <span>{{ t("link.notLoadedYet") }}</span>
    </p>
    <div v-else-if="status === 'error' && devices.length === 0" class="card__error" role="alert">
      <p class="card__alert">
        <HIcon name="alert" size="sm" />
        <span>{{ t("devices.loadFailed") }}</span>
      </p>
      <HButton variant="secondary" @click="emit('retry')">{{ t("common.retry") }}</HButton>
    </div>

    <EmptyState
      v-else-if="devices.length === 0"
      :title="t('devices.emptyTitle')"
      :text="t('devices.emptyText')"
      heading="h2"
    />

    <template v-else>
      <p v-if="!enrolled" class="card__note card__note--warn">
        <HIcon name="alert" size="sm" />
        <span>{{ t("devices.notEnrolled") }}</span>
      </p>
      <p v-if="atLimit" class="card__note">
        <HIcon name="info" size="sm" />
        <span>{{ t("devices.limit", { max }) }}</span>
      </p>
      <p v-if="showLimitHelp" class="card__help">{{ t("devices.limitHelp") }}</p>
      <p v-if="status === 'error' && isConnected" class="card__alert" role="alert">
        <HIcon name="alert" size="sm" />
        <span>{{ t("devices.loadFailed") }}</span>
        <HButton size="sm" variant="secondary" @click="emit('retry')">{{ t("common.retry") }}</HButton>
      </p>
      <div class="table-wrap">
        <table class="table">
          <caption class="sr-only">{{ t("devices.title") }}</caption>
          <thead>
            <tr>
              <th scope="col">{{ t("devices.colName") }}</th>
              <th scope="col">{{ t("devices.colLast") }}</th>
              <th scope="col"><span class="sr-only">{{ t("accounts.colActions") }}</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="device in sorted" :key="device.id" :data-device="device.name">
              <th scope="row">
                <div class="table__name">
                  {{ device.name }}
                  <HTag v-if="device.current" tone="accent">{{ t("devices.thisPc") }}</HTag>
                </div>
              </th>
              <td class="table__last" :data-label="t('devices.colLast')">{{ lastUse(device) }}</td>
              <td>
                <div class="table__actions">
                  <HButton
                    size="sm"
                    variant="danger"
                    tip-placement="end"
                    needs-link
                    :disabled="busy || device.current || !enrolled"
                    :hint="hintFor(device)"
                    :aria-label="named(device)"
                    @click="emit('remove', device)"
                  >
                    {{ t("devices.remove") }}
                  </HButton>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.card__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
}

.card__title {
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.card__counter {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  color: var(--tx2);
  font-family: var(--font-mono);
  font-size: var(--fs-body);
}

.card__wait {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  width: 100%;
  list-style: none;
}

.skeleton__row {
  height: var(--control-sm);
  border-radius: var(--radius-control);
  background: var(--card-2);
}

.card__loading,
.card__note,
.card__alert {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  color: var(--tx2);
  font-size: var(--fs-body);
}

.card__note--warn svg {
  color: var(--warn);
}

.card__alert {
  align-items: center;
  color: var(--tx);
}

.card__alert svg {
  color: var(--crit);
}

.card__error {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.card__help {
  color: var(--tx2);
  font-size: var(--fs-body);
}

/* Fenêtre étroite (sous 1 200 px) : défilement horizontal interne, comme le tableau des comptes. Au-delà
   rien ne rogne l'infobulle de « Retirer ». */
@media (max-width: 1199px) {
  .table-wrap {
    overflow-x: auto;
  }
}

.table {
  width: 100%;
  border-collapse: collapse;
}

.table th,
.table td {
  padding: var(--space-3) var(--space-2);
  border-bottom: var(--border-width) solid var(--bd);
  text-align: left;
  vertical-align: middle;
}

.table thead th {
  color: var(--tx2);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
  letter-spacing: var(--ls-label);
}

.table tbody tr:last-child th,
.table tbody tr:last-child td {
  border-bottom: 0;
}

.table__name {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-weight: var(--fw-semibold);
}

.table__last {
  color: var(--tx2);
}

.table__actions {
  display: flex;
  justify-content: flex-end;
}

/* Fenêtre plus étroite que le minimum prévu : chaque poste devient un bloc empilé. */
@media (max-width: 999px) {
  .table,
  .table tbody,
  .table tr,
  .table th,
  .table td {
    display: block;
  }

  .table thead {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
    clip-path: inset(50%);
  }

  .table tr {
    padding: var(--space-3) 0;
    border-bottom: var(--border-width) solid var(--bd);
  }

  .table th,
  .table td {
    padding: var(--space-1) 0;
    border-bottom: 0;
  }

  .table__last::before {
    content: attr(data-label) " : ";
  }

  .table__actions {
    justify-content: flex-start;
  }
}
</style>
