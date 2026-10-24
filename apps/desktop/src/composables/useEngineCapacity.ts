import { ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { getLinkBridge } from "@/link";
import type { EngineCapacity } from "@/resources";

/**
 * Les bornes de la machine pour les curseurs de ressources (HRT-69, HRT-102, BR-DEPLOY-038), lues sur
 * `GET /engine/capacity` dès que le formulaire peut parler au serveur. Partagé par le formulaire d'une image et par
 * l'installation d'une fiche. `onLoaded` reçoit les bornes lues : le formulaire y pose les valeurs de départ des
 * curseurs (un réglage déjà fait n'est pas écrasé par l'appelant). Un autre serveur a d'autres bornes : rien de
 * l'ancien ne reste, et une réponse en vol de l'ancien serveur n'atterrit nulle part.
 */
export function useEngineCapacity(
  serverId: () => string,
  enabled: () => boolean,
  onLoaded?: (capacity: EngineCapacity) => void,
) {
  const capacity = ref<EngineCapacity | null>(null);
  const failed = ref(false);
  let ticket = 0;

  async function load(): Promise<void> {
    const mine = ++ticket;
    try {
      const read = await getLinkBridge().readEngineCapacity(serverId());
      if (mine !== ticket) return;
      capacity.value = read;
      failed.value = read === null;
      if (read) onLoaded?.(read);
    } catch (error) {
      if (mine !== ticket) return;
      failed.value = true;
      logUiError(error, "deploy:capacity");
    }
  }

  watch(serverId, () => {
    ticket += 1;
    capacity.value = null;
    failed.value = false;
  });
  watch(
    [serverId, enabled],
    ([, on]) => {
      if (on && capacity.value === null) void load();
    },
    { immediate: true },
  );

  return { capacity, failed, load };
}
