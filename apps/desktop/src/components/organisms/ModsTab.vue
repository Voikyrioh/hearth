<script setup lang="ts">
import { computed, ref, useId, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import ModRow from "@/components/molecules/ModRow.vue";
import ModsPendingBar from "@/components/molecules/ModsPendingBar.vue";
import ModsApplyDialog from "@/components/organisms/ModsApplyDialog.vue";
import ModsCheckList from "@/components/organisms/ModsCheckList.vue";
import ModsDepositDialog from "@/components/organisms/ModsDepositDialog.vue";
import ModsPlayersList from "@/components/organisms/ModsPlayersList.vue";
import ModsProgress from "@/components/organisms/ModsProgress.vue";
import { readGameOnce } from "@/composables/useGameView";
import { useModsApply } from "@/composables/useModsApply";
import { useModsDeposit } from "@/composables/useModsDeposit";
import { useNow } from "@/composables/useNow";
import { useOnlinePlayers } from "@/composables/useOnlinePlayers";
import type { PendingAdd } from "@/composables/useServiceMods";
import { useServiceMods } from "@/composables/useServiceMods";
import { t } from "@/i18n";
import { MAX_BATCH_ADDS, type ModItem, type ServiceInfo } from "@/link";
import { addressesOf } from "@/services/game";
import {
  bannerItems,
  blockingBanner,
  dependentsNote,
  headingOf,
  noModsText,
  notesOf,
  ORIGIN_LEGEND,
  subheadingOf,
  versionLine,
} from "@/services/mods";
import { changeRows } from "@/services/modsApply";
import type { RightsGate } from "@/services/rights";
import { canAct } from "@/services/rights";
import { useServersStore } from "@/stores/servers";

// L'onglet « Mods » d'un serveur Minecraft installé par Hearth (HRT-261, HRT-262, BR-MOD-020 à 033, maquettes Main et
// SansMods) : ce que le serveur a comme mods, s'ils vont avec lui, d'où ils viennent, et le lot de gestes que le compte
// prépare (interrupteur « Activé », « Retirer », « Remettre »). Rien n'est écrit sur le serveur avant « Appliquer ». La
// liste est lisible par tout compte ; préparer un geste demande le droit de gérer les services (la raison est celle que
// le détail écrit déjà, une seule fois). Un gros serveur se lit en plusieurs fois : l'écran le dit et se complète seul.
// Un moteur sans mods le dit (Vanilla, plugins) et propose « Ajouter un serveur de jeu ». Tenu par `useServiceMods`.
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  gate: RightsGate;
  /** L'`id` de la raison du droit que le détail écrit déjà, une seule fois (BR-GRANT-014). */
  rightsReasonId: string;
  connected: boolean;
  /** L'agent parle à la console de ce serveur : la carte des joueurs peut exister (HRT-291). */
  consoleServed?: boolean;
  /** L'onglet « Console » existe pour ce serveur : « Ouvrir la console » est proposé (HRT-269). */
  consoleOpen?: boolean;
}>();
const emit = defineEmits<{
  /** Un lot non appliqué existe (ou n'existe plus) : le détail demande confirmation avant de quitter l'onglet. */
  dirty: [dirty: boolean];
  addServer: [];
  openConsole: [];
  openLogs: [];
}>();

const flow = useServiceMods(
  () => props.serverId,
  () => props.service,
  () => props.connected,
);
const titleId = useId();
const deposit = useModsDeposit(
  () => props.serverId,
  () => props.service.key,
);
/** L'écran montré : la liste, « Ajouter des mods » (dépôt), ou le contrôle des fichiers déposés. */
const screen = ref<"list" | "deposit" | "check" | "review" | "progress" | "players">("list");
/** L'adresse que les joueurs utilisent, lue quand « Ce que vos joueurs doivent installer » s'ouvre. */
const address = ref<string | null>(null);
const canWrite = computed(() => canAct(props.gate));
const room = computed(() => MAX_BATCH_ADDS - flow.adds.value.length);

// Le contrôle s'ouvre dès que l'agent l'a rendu.
watch(
  () => deposit.check.value.kind,
  (kind) => {
    if (kind === "done") screen.value = "check";
  },
);

// Appliquer (HRT-263) : la vérification, puis le suivi de l'opération et son issue.
const apply = useModsApply(
  () => props.serverId,
  () => props.service,
  () => void flow.reload(),
);
const clock = useNow();
const running = computed(
  () => props.service.state === "running" || props.service.state === "restart_loop",
);
const players = useOnlinePlayers(
  () => props.serverId,
  () => (props.consoleServed === true && screen.value === "review" ? props.service.key : null),
);
const rows = computed(() =>
  view.value
    ? changeRows({ batch: flow.batch.value, adds: flow.adds.value, view: view.value })
    : [],
);

function openReview() {
  if (flow.changeCount.value === 0) return;
  screen.value = "review";
}

/** L'agent a accepté le lot : le suivi prend la place, le lot est vidé (il est parti). */
function launched() {
  if (apply.launch.value === null && !apply.unknownResult.value) return;
  if (apply.launch.value !== null) {
    flow.clearAll();
    deposit.reset();
  }
  screen.value = "progress";
}

/** « Voir ce que vos joueurs doivent installer » : l'issue est dite, la liste se relit, l'adresse se lit. */
async function openPlayers() {
  apply.dismiss();
  screen.value = "players";
  address.value = null;
  void flow.reload();
  const game = await readGameOnce(props.serverId, props.service.key);
  if (game.kind !== "found") return;
  const servers = useServersStore();
  const addresses = addressesOf(
    servers.byId(props.serverId)?.host ?? "",
    game.view.ports,
    game.view.publicAddress,
    game.view.lanAddress,
    game.view.detectedAddress,
  );
  address.value = addresses?.internet?.text ?? addresses?.lan ?? null;
}

function closeProgress() {
  screen.value = "list";
  void flow.reload();
}

function leaveDeposit() {
  screen.value = "list";
}

function continueWith(entries: PendingAdd[]) {
  flow.addEntries(entries);
  deposit.reset();
  screen.value = "list";
}

function cancelAll() {
  flow.clearAll();
  deposit.reset();
}

/** Le fichier d'un ajout est un mod « posé » : la ligne le montre comme un mod actif, jamais encore sur le serveur. */
function itemOfAdd(add: PendingAdd): ModItem {
  return {
    file: add.targetFile,
    rank: 0,
    place: "active",
    enabled: true,
    origin: "hearth",
    readableName: true,
    identity: add.identity,
    verdict: add.verdict ?? { code: "compatible", missing: [], neededBy: [] },
  };
}

function labelOfAdd(add: PendingAdd): string {
  return add.replace === null
    ? t("mods.pending.add")
    : t("mods.pending.replace", { file: add.replace });
}

function labelOfReplaced(item: ModItem): string | undefined {
  const add = item.file === null ? undefined : flow.replacedBy.value.get(item.file);
  if (!add) return undefined;
  return t("mods.pending.replacedBy", { version: add.identity?.version ?? add.targetFile });
}

watch(
  () => flow.changeCount.value > 0,
  (dirty) => emit("dirty", dirty),
);

const view = flow.view;
const active = computed(() => view.value?.mods.filter((mod) => mod.place === "active") ?? []);
const retired = computed(() => view.value?.mods.filter((mod) => mod.place === "retired") ?? []);
const banners = computed(() => (view.value ? bannerItems(view.value) : { items: [], more: 0 }));
const sentenceOnly = computed(() => (view.value ? noModsText(view.value.applicable) : null));
const notes = computed(() => (view.value ? notesOf(view.value.game) : []));
const unread = computed(() => view.value?.unread ?? 0);

function noteOf(item: ModItem): string | null {
  const current = view.value;
  if (current === null || item.file === null) return null;
  const refused = flow.refusedNote.value.get(item.file);
  if (refused) return refused;
  const gesture = flow.pending.value.get(item.file);
  if (gesture === "disable" || gesture === "retire") {
    return dependentsNote(item, current, flow.pending.value);
  }
  return null;
}

function pendingOf(item: ModItem) {
  if (item.file === null) return undefined;
  if (flow.replacedBy.value.has(item.file)) return "replaced" as const;
  return flow.pending.value.get(item.file);
}

function keyOf(item: ModItem): string {
  return `${item.place}-${item.rank}-${item.file ?? ""}`;
}
</script>

<template>
  <div class="mt" data-mods-tab>
    <div v-if="flow.list.value.kind === 'loading'" class="mt__loading" role="status" :aria-label="t('mods.reading')" data-mods-loading>
      <HSkeleton v-for="n in 5" :key="n" size="lg" />
    </div>

    <div v-else-if="flow.list.value.kind === 'failed'" class="mt__state" data-mods-failed>
      <Callout tone="error" role="alert">{{ t("mods.failed") }}</Callout>
      <HButton variant="secondary" :disabled="!connected" data-mods-retry @click="flow.reload()">
        {{ t("mods.retry") }}
      </HButton>
    </div>

    <Callout v-else-if="flow.list.value.kind === 'refused'" tone="warn" data-mods-refused>
      {{
        flow.list.value.refusal.kind === "folder_blocked" ? t("mods.folderBlocked") : t("mods.gone")
      }}
    </Callout>

    <EmptyState
      v-else-if="sentenceOnly"
      :title="sentenceOnly.title"
      :text="sentenceOnly.text"
      heading="h3"
      data-mods-none
    >
      <template v-if="sentenceOnly.offer" #action>
        <HButton variant="primary" data-mods-add-server @click="emit('addServer')">
          {{ t("mods.addServer") }}
        </HButton>
      </template>
    </EmptyState>

    <ModsDepositDialog
      v-else-if="view && screen === 'deposit'"
      :server-id="serverId"
      :deposit="deposit"
      :can-write="canWrite"
      :connected="connected"
      @back="leaveDeposit"
    />

    <ModsCheckList
      v-else-if="view && screen === 'check' && deposit.check.value.kind === 'done'"
      :files="deposit.check.value.files"
      :game="view.game"
      :name-of="deposit.nameOfDeposit"
      :sha-of="deposit.shaOf"
      :room="room"
      @continue="continueWith"
      @more="screen = 'deposit'"
      @drop="deposit.dropChecked"
    />

    <ModsApplyDialog
      v-else-if="view && screen === 'review'"
      :server-id="serverId"
      :batch="flow.batch.value"
      :rows="rows"
      :running="running"
      :apply="apply"
      :players="players.state.value"
      :players-refreshing="players.refreshing.value"
      :now="clock"
      :console-open="consoleOpen === true"
      @back="screen = 'list'"
      @refresh-players="players.refresh()"
      @open-console="emit('openConsole')"
      @launched="launched"
    />

    <ModsPlayersList
      v-else-if="view && screen === 'players'"
      :view="view"
      :address="address"
      @back="screen = 'list'"
    />

    <ModsProgress
      v-else-if="screen === 'progress'"
      :server-id="serverId"
      :apply="apply"
      :view="view"
      @close="closeProgress"
      @players="openPlayers"
      @logs="emit('openLogs')"
    />

    <template v-else-if="view">
      <header class="mt__head">
        <div class="mt__heading">
          <h3 :id="titleId" class="mt__title" data-mods-heading>{{ headingOf(view) }}</h3>
          <p class="mt__sub" data-mods-sub>{{ subheadingOf(view) }}</p>
          <p class="mt__version" data-mods-version>{{ versionLine(view.game) }}</p>
        </div>
        <HButton
          variant="primary"
          needs-link
          :disabled="!canWrite"
          :described-by="canWrite ? undefined : rightsReasonId"
          data-mods-add
          @click="screen = 'deposit'"
        >
          <HIcon name="plus" size="sm" />
          {{ t("mods.add") }}
        </HButton>
      </header>

      <Callout v-for="line in notes" :key="line" tone="info" data-mods-note>{{ line }}</Callout>

      <section v-if="banners.items.length > 0" class="mt__banners" data-mods-banners>
        <Callout
          v-for="item in banners.items"
          :key="keyOf(item)"
          tone="error"
          role="alert"
          data-mods-banner
        >
          {{ blockingBanner(item, view.game) }}
        </Callout>
        <p v-if="banners.more > 0" class="mt__more" data-mods-banner-more>
          {{ t("mods.banner.more", { n: banners.more }) }}
        </p>
      </section>

      <ModsPendingBar
        v-if="flow.changeCount.value > 0"
        :count="flow.changeCount.value"
        :blocked-reason="unread > 0 ? t('mods.bar.unread') : undefined"
        @cancel-all="cancelAll"
        @review="openReview"
      />

      <Callout v-if="unread > 0" tone="info" data-mods-reading>
        <strong>{{ t("mods.reading") }}</strong>
        {{ t(unread === 1 ? "mods.readingTextOne" : "mods.readingText", { n: unread }) }}
      </Callout>
      <Callout v-if="view.truncated" tone="warn" data-mods-truncated>{{ t("mods.truncated") }}</Callout>

      <div class="mt__table-wrap">
        <table class="mt__table" :aria-labelledby="titleId" data-mods-table>
          <thead>
            <tr>
              <th scope="col">{{ t("mods.table.mod") }}</th>
              <th scope="col">{{ t("mods.table.version") }}</th>
              <th scope="col">{{ t("mods.table.loader") }}</th>
              <th scope="col">{{ t("mods.table.target") }}</th>
              <th scope="col">{{ t("mods.table.state") }}</th>
              <th scope="col">{{ t("mods.table.origin") }}</th>
              <th scope="col" class="mt__center">{{ t("mods.table.enabled") }}</th>
              <th scope="col" class="mt__right">{{ t("mods.table.action") }}</th>
            </tr>
          </thead>
          <tbody v-if="flow.adds.value.length > 0" data-mods-adds>
            <ModRow
              v-for="add in flow.adds.value"
              :key="add.deposit"
              :item="itemOfAdd(add)"
              :game="view.game"
              :view="view"
              :gate="gate"
              :rights-reason-id="rightsReasonId"
              pending="add"
              :pending-label="labelOfAdd(add)"
              :link-down="!connected"
              @undo="flow.removeAdd(add.deposit)"
            />
          </tbody>
          <tbody>
            <ModRow
              v-for="item in active"
              :key="keyOf(item)"
              :item="item"
              :game="view.game"
              :view="view"
              :gate="gate"
              :rights-reason-id="rightsReasonId"
              :pending="pendingOf(item)"
              :pending-label="labelOfReplaced(item)"
              :note="noteOf(item)"
              :link-down="!connected"
              @toggle="flow.toggle(item)"
              @retire="flow.retire(item)"
              @undo="flow.undo(item)"
            />
          </tbody>
          <tbody v-if="retired.length > 0" data-mods-retired>
            <tr class="mt__group">
              <th scope="rowgroup" colspan="8">{{ t("mods.table.retired") }}</th>
            </tr>
            <ModRow
              v-for="item in retired"
              :key="keyOf(item)"
              :item="item"
              :game="view.game"
              :view="view"
              :gate="gate"
              :rights-reason-id="rightsReasonId"
              :pending="item.file ? flow.pending.value.get(item.file) : undefined"
              :link-down="!connected"
              @restore="flow.restore(item)"
              @undo="flow.undo(item)"
            />
          </tbody>
        </table>
      </div>

      <p v-if="active.length === 0" class="mt__empty" data-mods-empty>{{ t("mods.subheadingNone") }}</p>

      <ul class="mt__legend" data-mods-legend>
        <li v-for="entry in ORIGIN_LEGEND" :key="entry.origin">
          <strong>{{ t(entry.term) }}</strong> : {{ t(entry.text) }}
        </li>
      </ul>
      <p class="mt__help" data-mods-help>{{ t("mods.help") }}</p>
      <p class="mt__warning" data-mods-warning>{{ t("mods.warning") }}</p>
    </template>
  </div>
</template>

<style scoped>
.mt {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.mt__loading,
.mt__state,
.mt__banners {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.mt__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
}

.mt__heading {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.mt__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.mt__sub,
.mt__version,
.mt__more,
.mt__help,
.mt__warning,
.mt__empty {
  margin: 0;
  color: var(--tx-secondary);
}

.mt__help,
.mt__legend,
.mt__more {
  font-size: var(--fs-small);
}

.mt__warning {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.mt__table-wrap {
  overflow-x: auto;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.mt__table {
  width: 100%;
  border-collapse: collapse;
}

.mt__table th {
  padding: var(--space-2) var(--space-3);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
  text-align: left;
}

.mt__center {
  text-align: center;
}

.mt__right {
  text-align: right;
}

.mt__group th {
  padding-top: var(--space-4);
  color: var(--tx);
}

.mt__legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-5);
  margin: 0;
  padding: 0;
  color: var(--tx-secondary);
  list-style: none;
}
</style>
