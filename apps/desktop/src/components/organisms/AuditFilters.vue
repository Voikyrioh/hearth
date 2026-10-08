<script setup lang="ts">
import { computed } from "vue";
import { type FilterDraft, type Period, periodError } from "@/audit/filters";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import MultiSelect from "@/components/molecules/MultiSelect.vue";
import { t } from "@/i18n";
import { AUDIT_KINDS, AUDIT_OUTCOMES, type AuditKind, type AuditOutcome } from "@/link";

// Filtres du journal (BR-AUDIT-014, 015, 016) : on les édite ici ; la page les applique au choix
// (la recherche peu après la frappe, une période personnalisée dès que ses deux dates sont valides).
// Le filtrage lui-même est fait par l'agent : ce composant ne filtre rien.
const props = defineProps<{
  draft: FilterDraft;
  /** Comptes proposés (ceux que le journal a montrés). */
  accounts: readonly string[];
  /** Au moins un filtre ou une recherche est appliqué ou édité : « Effacer » apparaît. */
  active: boolean;
  busy: boolean;
  now: number;
}>();

const emit = defineEmits<{
  "update:draft": [draft: FilterDraft];
  apply: [];
  clear: [];
}>();

const KIND_KEYS = {
  login_ok: "audit.kindLoginOk",
  login_denied: "audit.kindLoginDenied",
  accounts: "audit.kindAccounts",
  update: "audit.kindUpdate",
  denied: "audit.kindDenied",
} as const;
const OUTCOME_KEYS = {
  ok: "audit.outcomeOk",
  denied: "audit.outcomeDenied",
  failed: "audit.outcomeFailed",
} as const;

const kindOptions = AUDIT_KINDS.map((value) => ({ value, label: t(KIND_KEYS[value]) }));
const outcomeOptions = AUDIT_OUTCOMES.map((value) => ({ value, label: t(OUTCOME_KEYS[value]) }));
const periodOptions: { value: Period; label: string }[] = [
  { value: "all", label: t("audit.periodAll") },
  { value: "today", label: t("audit.periodToday") },
  { value: "week", label: t("audit.periodWeek") },
  { value: "month", label: t("audit.periodMonth") },
  { value: "custom", label: t("audit.periodCustom") },
];
const accountOptions = computed(() =>
  // Les comptes déjà choisis restent proposés même si le journal ne les montre plus.
  [...new Set([...props.accounts, ...props.draft.accounts])]
    .sort((a, b) => a.localeCompare(b))
    .map((value) => ({ value, label: value })),
);

const error = computed(() => periodError(props.draft, props.now));
const errorText = computed(() => {
  switch (error.value) {
    case "reversed":
      return t("audit.periodReversed");
    case "tooOld":
      return t("audit.periodTooOld");
    case "incomplete":
      return t("audit.periodIncomplete");
    default:
      return undefined;
  }
});

function patch(change: Partial<FilterDraft>) {
  emit("update:draft", { ...props.draft, ...change });
}
</script>

<template>
  <form class="filters" role="search" :aria-label="t('audit.filtersTitle')" @submit.prevent="emit('apply')">
    <h2 class="filters__title">{{ t("audit.filtersTitle") }}</h2>
    <div class="filters__row">
      <div class="filters__search">
        <HInput
          type="search"
          :model-value="draft.text"
          :label="t('audit.search')"
          :placeholder="t('audit.searchPlaceholder')"
          autocomplete="off"
          @update:model-value="(text: string) => patch({ text })"
        />
      </div>
      <MultiSelect
        class="filters__field"
        :model-value="draft.accounts"
        :label="t('audit.account')"
        :options="accountOptions"
        @update:model-value="(accounts: string[]) => patch({ accounts })"
      />
      <MultiSelect
        class="filters__field"
        :model-value="draft.kinds"
        :label="t('audit.kind')"
        :options="kindOptions"
        @update:model-value="(kinds: AuditKind[]) => patch({ kinds })"
      />
      <MultiSelect
        class="filters__field"
        :model-value="draft.outcomes"
        :label="t('audit.outcome')"
        :options="outcomeOptions"
        @update:model-value="(outcomes: AuditOutcome[]) => patch({ outcomes })"
      />
      <HSelect
        class="filters__field"
        :model-value="draft.period"
        :label="t('audit.period')"
        :options="periodOptions"
        @update:model-value="(period: Period) => patch({ period })"
      />
      <HButton
        v-if="active"
        class="filters__clear"
        variant="ghost"
        :disabled="busy"
        :aria-label="t('audit.clear')"
        @click="emit('clear')"
      >
        <span class="filters__clear-full" aria-hidden="true">{{ t("audit.clear") }}</span>
        <span class="filters__clear-short" aria-hidden="true">{{ t("audit.clearShort") }}</span>
      </HButton>
    </div>
    <div class="filters__row filters__row--end">
      <template v-if="draft.period === 'custom'">
        <div class="filters__field">
          <HInput
            type="date"
            :model-value="draft.fromDate"
            :label="t('audit.from')"
            :error="error ? errorText : undefined"
            @update:model-value="(fromDate: string) => patch({ fromDate })"
          />
        </div>
        <div class="filters__field">
          <HInput
            type="date"
            :model-value="draft.toDate"
            :label="t('audit.to')"
            @update:model-value="(toDate: string) => patch({ toDate })"
          />
        </div>
      </template>
    </div>
  </form>
</template>

<style scoped>
.filters {
  container: filters / inline-size;
  /* FIX:01M4D4H25X7PN4SH2N7DC3REGS : filtres sur une ligne, bouton compris (revue UX C29) */
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-4);
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.filters__title {
  flex: 0 0 100%;
  color: var(--tx2);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  letter-spacing: var(--ls-label);
  text-transform: uppercase;
}

.filters__row {
  display: contents;
}


.filters__search {
  flex: 2 1 calc(var(--audit-search-min) / 2);
  min-width: var(--audit-search-floor);
}

.filters__clear {
  flex: 0 0 auto;
}

.filters__clear-short {
  display: none;
}

/* Carte étroite : « Effacer » (le nom accessible reste « Effacer les filtres ») pour que tout tienne sur une rangée. */
@container filters (max-width: 820px) {
  .filters__clear-full {
    display: none;
  }

  .filters__clear-short {
    display: inline;
  }
}

.filters__field {
  flex: 1 1 calc(var(--audit-field-min) / 2);
  min-width: 0;
}

/* Une liste choisie n'est jamais coupée (« Tout l'hist ») : elle prend au moins la largeur de son plus long libellé,
   la rangée passe à la ligne si elle manque de place. */
.filters__field:has(select) {
  min-width: min-content;
}

.filters__field :deep(.select__input) {
  width: auto;
  min-width: 100%;
}
</style>
