<script setup lang="ts">
import { computed, inject, nextTick, ref, watch } from "vue";
import { routerKey } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import ReauthField from "@/components/molecules/ReauthField.vue";
import { type ActReport, useReauth } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { AdminActKind, Role } from "@/link";

// LA fenêtre de confirmation d'un acte d'administration (HRT-30, BR-TRUST-036, 042, 043), réutilisée par
// TOUS les actes : créer un compte, changer un rôle, le mot de passe d'un autre ou le sien, supprimer un
// compte, fermer des sessions, mettre à jour l'agent, mode attaque, réglage de fréquence. Le slot reçoit
// les champs propres à l'acte ; la fenêtre ajoute « Ton mot de passe » SELON CE QUE L'AGENT ANNONCE, lu à
// chaque ouverture (jamais deviné) :
// - agent qui n'annonce pas la confirmation : la liaison n'envoie aucun acte, la fenêtre dit de mettre
//   l'agent à jour et ne propose rien ;
// - ce PC sans clé au coffre : rien ne peut partir, la fenêtre l'explique et propose de se reconnecter
//   pour enregistrer ce poste (le mot de passe ne sert alors à rien) ;
// - délai de 5 minutes ouvert ET acte couvert : pas de champ, le temps restant est écrit ;
// - sinon : le champ.
// La preuve de la clé de ce PC est faite par la coquille, sans geste. Le champ est vidé après CHAQUE envoi
// et à chaque ouverture ; le mot de passe n'est jamais gardé (ni store, ni état au-delà du champ).
// Un refus se montre DANS la fenêtre : mot de passe faux ou attente sous le champ ; délai fermé entre-temps
// (`password_required`) : la fenêtre redemande le mot de passe SANS perdre la saisie de l'acte ; les
// autres refus : le texte de l'acte (`refusalText`). Fait ou résultat inconnu : la fenêtre se ferme
// (jamais rejoué, BR-RESIL-009). Échec déjà notifié : elle reste ouverte.

const props = withDefaults(
  defineProps<{
    open: boolean;
    serverId: string;
    /** Le genre d'acte : il décide si le délai de 5 minutes le couvre (règle de l'agent). */
    kind: AdminActKind;
    /** Le rôle visé par l'acte (création, changement de rôle) : donner « Administrateur » n'est jamais couvert. */
    role?: Role | null;
    title: string;
    submitLabel: string;
    /** Les champs de l'acte sont prêts (la fenêtre ajoute ses propres conditions). */
    canSubmit?: boolean;
    destructive?: boolean;
    /** Le libellé du champ quand le mot de passe de confirmation a un nom propre (« Ancien mot de passe »). */
    passwordLabel?: string;
    /** Lance l'acte avec le mot de passe saisi (`null` : délai ouvert). */
    perform: (adminPassword: string | null) => Promise<ActReport>;
    /** Le texte d'un refus qui n'est pas celui de la confirmation ; `undefined` : la fenêtre n'en montre pas. */
    refusalText?: (refusal: { kind: string; retry_after_s?: number }) => string | undefined;
  }>(),
  {
    role: null,
    canSubmit: true,
    destructive: false,
    passwordLabel: undefined,
    refusalText: undefined,
  },
);
const emit = defineEmits<{ close: [] }>();

const reauth = useReauth(
  () => props.serverId,
  () => props.kind,
  () => props.role,
);
const router = inject(routerKey, null);
const password = ref("");
const passwordError = ref<string | undefined>();
const error = ref<string | undefined>();
const notice = ref<string | undefined>();
const sending = ref(false);
const field = ref<InstanceType<typeof ReauthField> | null>(null);
const acts = ref<HTMLElement | null>(null);

watch(
  () => props.open,
  (open) => {
    // Un mot de passe ne reste dans aucun champ une fois la fenêtre fermée.
    password.value = "";
    passwordError.value = undefined;
    error.value = undefined;
    notice.value = undefined;
    if (open) void reauth.load();
  },
  { immediate: true },
);
watch(password, (value) => {
  // L'effacement après l'envoi n'efface pas l'erreur qu'il vient de provoquer.
  if (value !== "") passwordError.value = undefined;
});

// Où va le curseur une fois l'état lu (FIX:01M4D0RHZE7JFMV700JKA3DM1R, C17, C56) : dans le PREMIER champ à
// remplir de la fenêtre (identifiant, nouveau mot de passe…), jamais dans la confirmation qui vient à la fin ;
// une fenêtre sans champ d'acte (confirmation simple) met le curseur dans « Ton mot de passe ». Le curseur
// n'est déplacé que s'il n'est dans aucun champ : ce que l'utilisateur a déjà commencé à taper ne bouge pas.
watch(
  () => props.open && reauth.ready.value && !reauth.keyMissing.value && !reauth.agentTooOld.value,
  async (ready) => {
    if (!ready) return;
    await nextTick();
    const active = document.activeElement;
    if (active instanceof HTMLInputElement || active instanceof HTMLSelectElement) return;
    const first = firstActField();
    if (first) first.focus();
    else if (reauth.needsPassword.value) field.value?.focus();
  },
);

/** Le premier champ de saisie de l'acte (hors confirmation, qui est toujours le dernier). */
function firstActField(): HTMLElement | null {
  const form = acts.value?.closest("fieldset");
  const candidates = form?.querySelectorAll<HTMLElement>(
    "input:not([type=hidden]), select, textarea",
  );
  for (const element of candidates ?? []) {
    if (element.closest("[data-reauth-field]")) continue;
    if (element instanceof HTMLInputElement && element.type === "checkbox") continue;
    return element;
  }
  return null;
}

// FIX:01M4E5D3N2QCVCD494WC6S8WRD (C40, C41) : quand l'action est IMPOSSIBLE, la fenêtre dit l'état vrai en titre (plus la
// question de l'acte), ne propose aucun bouton d'action, et mène là où ça se règle.
const blocked = computed(() => reauth.agentTooOld.value || reauth.keyMissing.value);
const shownTitle = computed(() => {
  if (reauth.agentTooOld.value) return t("reauth.agentOldTitle");
  if (reauth.keyMissing.value) return t("reauth.noKeyTitle");
  return props.title;
});

/** Mène à la page de mise à jour de l'agent (Réglages) et ferme la fenêtre. */
async function goToSettings() {
  emit("close");
  await router?.push({ name: "settings" });
}

const awaiting = computed(() => !reauth.ready.value && !reauth.failed.value);
const askPassword = computed(
  () => reauth.ready.value && !reauth.keyMissing.value && reauth.needsPassword.value,
);
const canSend = computed(
  () =>
    props.canSubmit &&
    reauth.ready.value &&
    !reauth.keyMissing.value &&
    !reauth.agentTooOld.value &&
    (!reauth.needsPassword.value || password.value !== ""),
);

async function submit() {
  if (sending.value) return;
  error.value = undefined;
  passwordError.value = undefined;
  notice.value = undefined;
  // Sous le délai, pour un acte couvert, aucun mot de passe n'est envoyé ; sinon celui du champ.
  const typed = reauth.needsPassword.value ? password.value : null;
  sending.value = true;
  let report: ActReport;
  try {
    report = await props.perform(typed);
  } finally {
    sending.value = false;
    // Vidé après CHAQUE envoi, réussi ou non.
    password.value = "";
  }
  if (report.kind === "done" || report.kind === "unknown") {
    emit("close");
    return;
  }
  // Refusé ou échoué : le délai a pu se fermer côté agent (un mot de passe faux le ferme), l'état se relit.
  void reauth.load();
  if (report.kind !== "refused" || !report.refusal) return;
  const refusal = report.refusal;
  // Le curseur revient dans la confirmation vidée (le champ était figé pendant l'envoi) : le prochain
  // geste est de la retaper, sans clic.
  void nextTick().then(() => field.value?.focus());
  switch (refusal.kind) {
    case "wrong_password":
      passwordError.value = t("reauth.wrongPassword");
      break;
    case "too_many_attempts":
      passwordError.value = t("reauth.waiting", { n: refusal.retry_after_s ?? 60 });
      break;
    case "password_required":
      notice.value = t("reauth.elapsed");
      break;
    case "busy":
      error.value = t("reauth.busy");
      break;
    default:
      error.value = props.refusalText?.(refusal);
  }
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="shownTitle"
    :submit-label="submitLabel"
    :hide-submit="blocked"
    :can-submit="canSend"
    :busy="sending"
    :error="error"
    :destructive="destructive"
    :cancel-label="reauth.keyMissing.value || reauth.agentTooOld.value ? t('common.close') : undefined"
    @submit="submit"
    @cancel="emit('close')"
  >
    <div v-if="awaiting" class="admin__wait" data-reauth-loading>
      <HSpinner :label="t('reauth.loading')" />
    </div>
    <div v-else-if="reauth.failed.value" class="admin__state" data-reauth-failed>
      <p role="alert" class="admin__alert">{{ t("reauth.stateFailed") }}</p>
      <HButton variant="secondary" @click="reauth.load()">{{ t("common.retry") }}</HButton>
    </div>
    <div v-else-if="reauth.agentTooOld.value" class="admin__state" data-reauth-agent-old>
      <p role="alert" class="admin__alert">{{ t("failure.agentTooOld") }}</p>
      <HButton variant="secondary" data-reauth-settings @click="goToSettings">
        {{ t("reauth.goSettings") }}
      </HButton>
    </div>
    <div v-else-if="reauth.keyMissing.value" class="admin__state" data-reauth-no-key>
      <p class="admin__help">{{ t("reauth.noKey") }}</p>
      <HButton variant="secondary" data-reauth-reconnect @click="reauth.reconnect()">
        {{ t("reauth.reconnect") }}
      </HButton>
    </div>
    <template v-else>
      <div ref="acts" class="admin__acts">
        <slot />
      </div>
      <p v-if="notice" class="admin__notice" role="status" data-reauth-notice>{{ notice }}</p>
      <p v-if="reauth.elevated.value" class="admin__help" data-reauth-elevated>
        {{ t("reauth.elevated", { time: reauth.clock.value }) }}
      </p>
      <ReauthField
        v-else-if="askPassword"
        ref="field"
        v-model="password"
        :label="passwordLabel"
        :help="notice ? undefined : t('reauth.help')"
        :error="passwordError"
      />
      <slot name="after" />
    </template>
  </FormDialog>
</template>

<style scoped>
.admin__acts {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.admin__wait {
  display: flex;
  justify-content: center;
  padding: var(--space-4) 0;
}

.admin__state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.admin__title {
  font-weight: var(--fw-semibold);
}

.admin__help {
  color: var(--tx2);
}

.admin__alert {
  color: var(--crit);
}

.admin__notice {
  color: var(--warn);
}
</style>
