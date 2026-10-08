import { computed, ref, watch } from "vue";
import { reportUiError } from "@/errors/report";
import { type AdminActKind, getLinkBridge, type ReauthState, type Role } from "@/link";
import { useCountdown } from "./useCountdown";

/**
 * Ce que l'acte rend à la fenêtre de confirmation (`AccountReport`, `AttackModeReport`…) : fait, refusé
 * (avec le code du refus, jamais un texte de l'agent), résultat inconnu (le lien est tombé avant la
 * réponse : jamais rejoué), ou échec déjà notifié.
 */
export interface ActReport {
  kind: "done" | "refused" | "unknown" | "failed";
  refusal?: { kind: string; retry_after_s?: number };
}

/**
 * Ce qu'une fenêtre d'acte d'administration doit savoir de la confirmation (HRT-30, BR-TRUST-042, 043) :
 * lu de l'AGENT à chaque ouverture (l'interface ne devine ni sa capacité ni l'élévation de 5 minutes).
 *
 * - `supported` : l'agent annonce la confirmation des actes ; sinon `agentTooOld` : la liaison n'envoie
 *   aucun acte à cet agent, la fenêtre le dit et ne propose rien ;
 * - `keyMissing` : ce PC n'a pas de clé au coffre, rien ne peut partir d'ici : la fenêtre l'explique ;
 * - `elevated` : pour CET acte (la règle de couverture est celle de l'agent, `reauthCovers`) le mot de
 *   passe n'est pas demandé, il reste `remaining` secondes ;
 * - `needsPassword` : le champ est affiché.
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
  const covered = ref(false);
  const countdown = useCountdown();
  let reads = 0;
  let coverage = 0;

  /** Relit l'état à l'agent. Un échec n'invente rien : `failed`, et le champ reste exigé (le plus strict). */
  async function load(): Promise<void> {
    const mine = ++reads;
    failed.value = false;
    try {
      const next = await getLinkBridge().getReauthState(serverId());
      if (mine !== reads) return;
      state.value = next;
      countdown.start(next.elevatedForS);
    } catch (error) {
      if (mine !== reads) return;
      state.value = null;
      failed.value = true;
      countdown.stop();
      reportUiError(error, "reauth:state");
    }
  }

  watch(
    () => [kind(), role()] as const,
    async ([actKind, actRole]) => {
      const mine = ++coverage;
      try {
        const next = await getLinkBridge().reauthCovers(actKind, actRole);
        if (mine === coverage) covered.value = next;
      } catch (error) {
        if (mine === coverage) covered.value = false;
        reportUiError(error, "reauth:covers");
      }
    },
    { immediate: true },
  );

  const supported = computed(() => state.value?.supported === true);
  /** L'état est lu et l'agent n'annonce pas la confirmation : rien ne peut partir. */
  const agentTooOld = computed(() => state.value !== null && !supported.value);
  const keyMissing = computed(() => supported.value && state.value?.hasDeviceKey === false);
  const elevated = computed(
    () => supported.value && !keyMissing.value && covered.value && countdown.remaining.value > 0,
  );
  const needsPassword = computed(() => supported.value && !elevated.value);
  const ready = computed(() => state.value !== null);
  /** « 4:32 » : le temps qu'il reste, à la seconde. */
  const clock = computed(() => {
    const seconds = countdown.remaining.value;
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
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
    elevated,
    needsPassword,
    remaining: countdown.remaining,
    clock,
    load,
    reconnect,
  };
}
