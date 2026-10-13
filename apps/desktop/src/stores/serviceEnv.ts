import { defineStore } from "pinia";
import { ref, shallowRef } from "vue";
import { logUiError } from "@/errors/report";
import { type EnvVariable, failureOf, getLinkBridge } from "@/link";
import { useServiceRightsStore } from "./serviceRights";

/** Où en est la révélation : rien, lecture en cours, valeurs montrées, ou échec (valeurs toujours masquées). */
export type EnvPhase = "idle" | "reading" | "shown" | "failed";
/** Pourquoi une lecture a échoué : le droit a été retiré (BR-SVC-009), ou simplement illisible. */
export type EnvFailure = "lost" | "unreadable";

/**
 * Les valeurs de variables d'UN SEUL service à la fois (HRT-63, BR-SVC-009). Ce magasin n'est JAMAIS persisté :
 * rien dans le disque, rien dans un journal, rien dans le presse-papiers. Les valeurs n'existent qu'ici, en
 * mémoire, du moment où le compte les demande jusqu'au remasquage. `clear` les EFFACE (le tableau est remplacé
 * par `null`, pas seulement caché). Une réponse tardive d'un service qu'on a quitté entre-temps est jetée sans
 * jamais être gardée. Aucune valeur, aucun nom n'entre dans une erreur ni dans un rapport : seul un texte fixe
 * est signalé.
 *
 * Le remasquage (fermeture du détail, autre service, lien coupé, service disparu, droit retiré) est demandé par
 * l'écran à travers `clear` ; ce magasin ne le devine pas.
 */
export const useServiceEnvStore = defineStore("serviceEnv", () => {
  const phase = ref<EnvPhase>("idle");
  const failure = ref<EnvFailure | null>(null);
  /** Les variables révélées, ou `null` : effacées. */
  const variables = shallowRef<EnvVariable[] | null>(null);
  /** Le service dont les valeurs sont (ou vont être) montrées. */
  const target = ref<{ serverId: string; key: string } | null>(null);
  let ticket = 0;

  /** Efface tout : les valeurs ne sont plus nulle part. */
  function clear(): void {
    ticket += 1;
    phase.value = "idle";
    failure.value = null;
    variables.value = null;
    target.value = null;
  }

  /** Demande les valeurs de ce service (une lecture consignée par l'agent). Sans effet si une lecture est en cours. */
  async function reveal(serverId: string, key: string): Promise<void> {
    if (phase.value === "reading") return;
    const mine = ++ticket;
    variables.value = null;
    failure.value = null;
    target.value = { serverId, key };
    phase.value = "reading";
    try {
      const read = await getLinkBridge().revealServiceEnv(serverId, key);
      // On a remasqué, changé de service ou quitté pendant la lecture : la réponse est jetée.
      if (mine !== ticket) return;
      variables.value = read;
      phase.value = "shown";
    } catch (error) {
      if (mine !== ticket) return;
      variables.value = null;
      phase.value = "failed";
      const kind = failureOf(error)?.kind;
      failure.value = kind === "missing_services_right" ? "lost" : "unreadable";
      if (kind === "missing_services_right") {
        // Le droit a été retiré entre-temps : on le relit (BR-GRANT-011) pour que les boutons suivent.
        void useServiceRightsStore().noteRefusal(serverId);
      } else if (!kind) {
        // Jamais le message d'origine : il pourrait citer ce qui revient du serveur.
        logUiError(new Error("lecture des valeurs en échec"), "serviceEnv:reveal");
      }
    }
  }

  return { phase, failure, variables, target, reveal, clear };
});
