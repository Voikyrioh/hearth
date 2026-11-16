import { computed, nextTick, ref, watch } from "vue";
import type ReauthField from "@/components/molecules/ReauthField.vue";
import { type ActReport, useReauth } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { AdminActKind, Role } from "@/link";

// La confirmation d'un acte d'administration, sans enveloppe (HRT-245, BR-TRUST-036, 042, 043) : la fenêtre
// `AdminActDialog` et la page « Vérifiez avant de lancer » (HRT-246) posent le MÊME panneau (`AdminActPanel`) sur ce
// MÊME état. Ce que l'agent annonce est lu à chaque ouverture (jamais deviné) :
// - agent qui n'annonce pas la confirmation : la liaison n'envoie aucun acte, le panneau dit de mettre l'agent à jour ;
// - ce PC sans clé au coffre : rien ne peut partir, le panneau l'explique et propose de se reconnecter ;
// - acte de pilotage d'un service en état normal (HRT-194) : pas de champ, rien n'est demandé ;
// - délai de 5 minutes ouvert ET acte couvert : pas de champ, le temps restant est écrit ;
// - sinon : le champ « Votre mot de passe ».
// Le champ est vidé après CHAQUE envoi et à chaque ouverture ; le mot de passe n'est jamais gardé (ni store, ni état
// au-delà du champ). Un refus se montre sous le champ ; délai fermé entre-temps (`password_required`) ou état passé de
// normal à alerte entre l'ouverture et l'envoi : le champ est demandé SANS perdre la saisie de l'acte. Fait, résultat
// inconnu ou droit perdu : `onFinished` (jamais rejoué, BR-RESIL-009). Échec déjà notifié : rien ne se ferme.

export interface AdminActOptions {
  serverId: () => string;
  /** Le genre d'acte : il décide ce que l'agent exige du mot de passe (règle de l'agent, `reauthPasswordNeed`). */
  kind: () => AdminActKind;
  /** Le rôle visé par l'acte (création, changement de rôle) : donner « Administrateur » n'est jamais couvert. */
  role: () => Role | null;
  /** Le panneau est visible : l'état de la confirmation est lu à chaque ouverture. */
  active: () => boolean;
  /** Les champs de l'acte sont prêts (le panneau ajoute ses propres conditions). */
  canSubmit: () => boolean;
  /** Ce que l'acte attend encore avant d'être possible, dit quand `canSubmit` est faux. */
  blockedReason: () => string | undefined;
  /** Une question à poser AVANT l'acte : `false` annule l'envoi sans rien perdre. */
  beforeSubmit?: () => Promise<boolean>;
  /** Lance l'acte avec le mot de passe saisi (`null` : délai ouvert). */
  perform: (adminPassword: string | null) => Promise<ActReport>;
  /** Le texte d'un refus qui n'est pas celui de la confirmation ; `undefined` : rien n'est montré. */
  refusalText?: (refusal: { kind: string; retry_after_s?: number }) => string | undefined;
  /** L'acte est fini (fait, résultat inconnu, droit perdu) : l'enveloppe se ferme ou la page avance. */
  onFinished: () => void;
}

export function useAdminAct(options: AdminActOptions) {
  const reauth = useReauth(options.serverId, options.kind, options.role);
  const password = ref("");
  const passwordError = ref<string | undefined>();
  const error = ref<string | undefined>();
  const notice = ref<string | undefined>();
  const sending = ref(false);
  const field = ref<InstanceType<typeof ReauthField> | null>(null);
  const acts = ref<HTMLElement | null>(null);

  watch(
    options.active,
    (open) => {
      // Un mot de passe ne reste dans aucun champ une fois le panneau fermé.
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

  // Où va le curseur une fois l'état lu (FIX:01M4D0RHZE7JFMV700JKA3DM1R, C17, C56) : dans le PREMIER champ à
  // remplir de l'acte (identifiant, nouveau mot de passe…), jamais dans la confirmation qui vient à la fin ; sans champ
  // d'acte (confirmation simple), le curseur va dans « Votre mot de passe ». Il n'est déplacé que s'il n'est dans
  // aucun champ : ce que l'utilisateur a commencé à taper ne bouge pas.
  watch(
    () =>
      options.active() &&
      reauth.ready.value &&
      !reauth.keyMissing.value &&
      !reauth.agentTooOld.value,
    async (ready) => {
      if (!ready) return;
      await nextTick();
      const active = document.activeElement;
      if (active instanceof HTMLInputElement || active instanceof HTMLSelectElement) return;
      const first = firstActField();
      // FIX:01M5PYVP4WFPG3YMVV68T041R4
      // Sans défilement : un récapitulatif s'ouvre en haut pour être relu de haut en bas (revue UX 2026-10-24, constat 15).
      if (first) first.focus({ preventScroll: true });
      else if (reauth.needsPassword.value) field.value?.focus({ preventScroll: true });
    },
  );

  // FIX:01M4E5D3N2QCVCD494WC6S8WRD (C40, C41) : quand l'action est IMPOSSIBLE, l'état vrai est dit en titre (plus la
  // question de l'acte), aucun bouton d'action n'est proposé, et on mène là où ça se règle.
  const blocked = computed(() => reauth.agentTooOld.value || reauth.keyMissing.value);
  const shownTitle = (title: string): string => {
    if (reauth.agentTooOld.value) return t("reauth.agentOldTitle");
    if (reauth.keyMissing.value) return t("reauth.noKeyTitle");
    return title;
  };

  const awaiting = computed(() => !reauth.ready.value && !reauth.failed.value);
  const askPassword = computed(
    () => reauth.ready.value && !reauth.keyMissing.value && reauth.needsPassword.value,
  );
  /** Le mot de passe manque : c'est tout ce qui retient le bouton (revue UX 2026-10-24, constat 13). */
  const needPasswordReason = computed(() => {
    // L'acte lui-même n'est pas prêt : sa raison d'abord (le nom à retaper, HRT-86) ; ensuite le mot de passe.
    if (!options.canSubmit()) return options.blockedReason();
    return askPassword.value && password.value === "" ? t("reauth.needPassword") : undefined;
  });
  const canSend = computed(
    () =>
      options.canSubmit() &&
      reauth.ready.value &&
      !reauth.keyMissing.value &&
      !reauth.agentTooOld.value &&
      (!reauth.needsPassword.value || password.value !== ""),
  );

  async function submit() {
    if (sending.value) return;
    // « Envoi en cours » dès le geste : un second appui pendant la question préalable (le contrat) ne part pas.
    sending.value = true;
    let proceed = false;
    try {
      proceed = !options.beforeSubmit || (await options.beforeSubmit());
    } finally {
      if (!proceed) sending.value = false;
    }
    if (!proceed) return;
    error.value = undefined;
    passwordError.value = undefined;
    notice.value = undefined;
    // Sous le délai, pour un acte couvert, aucun mot de passe n'est envoyé ; sinon celui du champ.
    const typed = reauth.needsPassword.value ? password.value : null;
    // Rien n'était demandé à l'ouverture : si l'agent le demande maintenant, c'est que l'état a changé.
    const askedNothing = reauth.need.value === "no";
    sending.value = true;
    let report: ActReport;
    try {
      report = await options.perform(typed);
    } finally {
      sending.value = false;
      // Vidé après CHAQUE envoi, réussi ou non.
      password.value = "";
    }
    // `right_lost` : le serveur a refusé faute du droit de gérer les services (retiré entre-temps, BR-GRANT-010) : le
    // texte et la relecture du droit sont déjà faits (`useServerAction`).
    if (report.kind === "done" || report.kind === "unknown" || report.kind === "right_lost") {
      options.onFinished();
      return;
    }
    // Refusé ou échoué : le délai a pu se fermer côté agent (un mot de passe faux le ferme), l'état se relit.
    void reauth.load();
    if (report.kind !== "refused" || !report.refusal) return;
    const refusal = report.refusal;
    // Le curseur revient dans la confirmation vidée (le champ était figé pendant l'envoi) : le prochain geste est de
    // la retaper, sans clic.
    void nextTick().then(() => field.value?.focus());
    switch (refusal.kind) {
      case "wrong_password":
        passwordError.value = t("reauth.wrongPassword");
        break;
      case "too_many_attempts":
        passwordError.value = t("reauth.waiting", { n: refusal.retry_after_s ?? 60 });
        break;
      case "password_required":
        notice.value = askedNothing ? t("reauth.nowRequired") : t("reauth.elapsed");
        break;
      case "busy":
        error.value = t("reauth.busy");
        break;
      default:
        error.value = options.refusalText?.(refusal);
    }
  }

  return {
    reauth,
    password,
    passwordError,
    error,
    notice,
    sending,
    field,
    acts,
    blocked,
    shownTitle,
    awaiting,
    askPassword,
    needPasswordReason,
    canSend,
    submit,
  };
}

export type AdminAct = ReturnType<typeof useAdminAct>;
