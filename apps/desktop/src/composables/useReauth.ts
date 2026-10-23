import { computed, ref, watch } from "vue";
import { reportUiError } from "@/errors/report";
import {
  type AdminActKind,
  getLinkBridge,
  type PasswordNeed,
  type ReauthState,
  type Role,
} from "@/link";
import { useCountdown } from "./useCountdown";

/**
 * Ce que l'acte rend à la fenêtre de confirmation (`AccountReport`, `AttackModeReport`…) : fait, refusé
 * (avec le code du refus, jamais un texte de l'agent), résultat inconnu (le lien est tombé avant la
 * réponse : jamais rejoué), ou échec déjà notifié.
 */
export interface ActReport {
  kind: "done" | "refused" | "unknown" | "failed" | "right_lost";
  refusal?: { kind: string; retry_after_s?: number };
}

/**
 * Ce qu'une fenêtre d'acte d'administration doit savoir de la confirmation (HRT-30, BR-TRUST-042, 043) :
 * lu de l'AGENT à chaque ouverture (l'interface ne devine ni sa capacité ni l'élévation de 5 minutes).
 *
 * - `supported` : l'agent annonce la confirmation des actes ; sinon `agentTooOld` : la liaison n'envoie
 *   aucun acte à cet agent, la fenêtre le dit et ne propose rien ;
 * - `keyMissing` : ce PC n'a pas de clé au coffre, rien ne peut partir d'ici : la fenêtre l'explique ;
 * - `need` : ce que l'agent exige du mot de passe pour CET acte dans l'état de confiance qu'il a annoncé
 *   (HRT-194, `reauthPasswordNeed` : la règle est celle de l'agent, jamais recopiée ici) : `no` (rien,
 *   piloter un service en état normal), `unless_elevated`, `yes` ; état non annoncé (agent d'avant) : la
 *   règle d'avant ; état inconnu : le plus strict ;
 * - `elevated` : le mot de passe n'est pas demandé pour CET acte parce que le délai de 5 minutes est ouvert,
 *   il reste `remaining` secondes ;
 * - `needsPassword` : le champ est affiché (jamais quand `need` vaut `no`).
 *
 * Rien n'est gardé dans un store : l'état vit le temps de la fenêtre. Le mot de passe n'est jamais ici.
 */
export function useReauth(
  serverId: () => string,
  kind: () => AdminActKind,
  role: () => Role | null = () => null,
) {
  const state = ref<ReauthState | null>(null);
  const failed = ref(false);
  /** Le plus strict tant que l'agent n'a pas répondu : jamais « pas de mot de passe » par défaut. */
  const need = ref<PasswordNeed>("yes");
  const countdown = useCountdown();
  let reads = 0;
  let needReads = 0;

  /** Demande à la coquille ce que l'agent exige pour cet acte, dans l'état de confiance annoncé. */
  async function refreshNeed(): Promise<void> {
    const mine = ++needReads;
    try {
      const next = await getLinkBridge().reauthPasswordNeed(
        kind(),
        role(),
        state.value?.trust ?? null,
      );
      if (mine === needReads) need.value = next;
    } catch (error) {
      if (mine === needReads) need.value = "yes";
      reportUiError(error, "reauth:need");
    }
  }

  /** Relit l'état à l'agent. Un échec n'invente rien : `failed`, et le champ reste exigé (le plus strict). */
  async function load(): Promise<void> {
    const mine = ++reads;
    failed.value = false;
    try {
      const next = await getLinkBridge().getReauthState(serverId());
      if (mine !== reads) return;
      state.value = next;
      countdown.start(next.elevatedForS);
      // L'état de confiance vient d'être lu : la règle se relit avec lui, avant que la fenêtre ne s'affiche.
      await refreshNeed();
    } catch (error) {
      if (mine !== reads) return;
      state.value = null;
      failed.value = true;
      countdown.stop();
      reportUiError(error, "reauth:state");
    }
  }

  watch(() => [kind(), role()] as const, refreshNeed, { immediate: true });

  const supported = computed(() => state.value?.supported === true);
  /** L'état est lu et l'agent n'annonce pas la confirmation : rien ne peut partir. */
  const agentTooOld = computed(() => state.value !== null && !supported.value);
  const keyMissing = computed(() => supported.value && state.value?.hasDeviceKey === false);
  const elevated = computed(
    () =>
      supported.value &&
      !keyMissing.value &&
      need.value === "unless_elevated" &&
      countdown.remaining.value > 0,
  );
  const needsPassword = computed(
    () =>
      supported.value &&
      (need.value === "yes" || (need.value === "unless_elevated" && !elevated.value)),
  );
  const ready = computed(() => state.value !== null);
  /** « 4 min 32 s » : le temps qu'il reste, avec ses unités. */
  const clock = computed(() => {
    const seconds = countdown.remaining.value;
    const minutes = Math.floor(seconds / 60);
    return minutes > 0 ? `${minutes} min ${seconds % 60} s` : `${seconds} s`;
  });

  /** Se reconnecter pour enregistrer ce poste : la déconnexion volontaire ramène la connexion par mot de passe. */
  async function reconnect(): Promise<void> {
    try {
      await getLinkBridge().logout(serverId());
    } catch (error) {
      reportUiError(error, "reauth:reconnect");
    }
  }

  return {
    state,
    failed,
    ready,
    supported,
    agentTooOld,
    keyMissing,
    need,
    elevated,
    needsPassword,
    remaining: countdown.remaining,
    clock,
    load,
    reconnect,
  };
}
