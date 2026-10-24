import { computed, onBeforeUnmount, type Ref, ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { type DeployCheck, type DeployCheckInput, getLinkBridge } from "@/link";

/** Le délai après la dernière frappe avant de demander au serveur (les constats sont indicatifs, BR-DEPLOY-032). */
export const CHECK_DELAY_MS = 400;

/**
 * Les constats du serveur sur ce que dit un formulaire de déploiement, en direct (HRT-69, HRT-102) : le nom, les
 * ports, l'espace sur le disque des images. Partagé par le formulaire d'une image et par l'installation d'une fiche :
 * les deux posent la MÊME question (`POST /deployments/check`) et jugent la réponse de la même façon. Tout est
 * INDICATIF : le serveur refait chaque contrôle au lancement avec la même sévérité (BR-DEPLOY-032).
 *
 * - une question ne porte JAMAIS un secret : ni variable, ni commande, ni réponse d'une fiche (BR-DEPLOY-033) ;
 * - une réponse en retard sur une plus récente est écartée ;
 * - une réponse qui ne correspond plus à la saisie n'est pas rendue (`check` vaut `null`).
 */
export function useDeployCheck(
  serverId: () => string,
  enabled: () => boolean,
  input: Ref<DeployCheckInput | null>,
) {
  const answer = ref<DeployCheck | null>(null);
  const sent = ref<string | null>(null);
  const checking = ref(false);
  let ticket = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** La consultation telle qu'elle partirait : sans secret, donc sûre à comparer. */
  const key = computed(() => (input.value ? JSON.stringify(input.value) : null));

  /** Oublie tout constat et toute réponse en vol. */
  function reset(): void {
    clearTimeout(timer);
    ticket += 1;
    answer.value = null;
    sent.value = null;
    checking.value = false;
  }

  /** Pose la question maintenant (sans attendre la fin de la frappe) et rend la réponse, `null` si elle n'a pas pu venir. */
  async function refresh(): Promise<DeployCheck | null> {
    clearTimeout(timer);
    const asked = input.value;
    const askedKey = key.value;
    if (!asked || !enabled()) {
      answer.value = null;
      sent.value = null;
      checking.value = false;
      return null;
    }
    const mine = ++ticket;
    checking.value = true;
    try {
      const reply = await getLinkBridge().checkDeployment(serverId(), asked);
      if (mine !== ticket) return null;
      answer.value = reply;
      sent.value = askedKey;
      return reply;
    } catch (error) {
      if (mine !== ticket) return null;
      // Une consultation qui échoue n'interdit rien : le serveur dira non au lancement s'il le faut.
      answer.value = null;
      sent.value = null;
      logUiError(error, "deploy:check");
      return null;
    } finally {
      if (mine === ticket) checking.value = false;
    }
  }

  watch(key, (now) => {
    clearTimeout(timer);
    if (now === null) {
      reset();
      return;
    }
    checking.value = true;
    timer = setTimeout(() => void refresh(), CHECK_DELAY_MS);
  });
  // Le lien ou le moteur reviennent : on repose la question.
  watch(enabled, (now) => {
    if (now && key.value !== null) void refresh();
  });
  // Un autre serveur a d'autres constats : rien de l'ancien ne reste, même si la clé de consultation est la même.
  watch(serverId, () => {
    reset();
    if (key.value !== null && enabled()) void refresh();
  });
  onBeforeUnmount(() => {
    clearTimeout(timer);
    ticket += 1;
  });

  return {
    /** Le constat du serveur est celui de ce que le formulaire dit MAINTENANT (pas d'une saisie plus ancienne). */
    check: computed(() => (sent.value === key.value ? answer.value : null)),
    checking,
    refresh,
    reset,
  };
}
