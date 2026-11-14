import { computed, onBeforeUnmount } from "vue";
import type { ServiceOperation } from "@/link";
import { useServiceOpsStore } from "@/stores/serviceOps";

/**
 * Le suivi d'UNE opération de service par une fenêtre (déployer HRT-69, supprimer HRT-86, arrêter un jeu HRT-104), tenu
 * à un seul endroit : la fenêtre suit l'opération elle-même, donc son issue n'est PAS annoncée par une notification
 * tant qu'elle la porte. Quatre gestes :
 *
 * - `begin` : l'agent a accepté l'opération, la fenêtre la suit (`watch` du magasin) ;
 * - `operation` / `finished` : l'opération telle qu'elle court, et sa dernière image une fois terminée ;
 * - `settle` : la fenêtre a dit l'issue, elle est acquittée auprès de la coquille (jamais rejouée) ;
 * - `release` : la fenêtre se ferme AVANT la fin, l'issue sera annoncée comme les autres (notification, bande).
 *
 * La page qui se ferme avec une fenêtre de suivi ouverte la relâche d'elle-même. Aucune règle de statut ici : l'agent
 * les décide, l'appelant dit l'issue à sa façon.
 *
 * @param serverId le serveur de l'opération.
 * @param operationId l'identifiant de l'opération suivie, ou `null` (aucune, ou lien tombé avant la réponse).
 */
export function useOperationFollow(serverId: () => string, operationId: () => string | null) {
  const operations = useServiceOpsStore();

  const tracked = computed(() => {
    const id = operationId();
    return id ? operations.get(serverId(), id) : undefined;
  });
  /** L'opération telle qu'elle court (étape, progression, décision attendue, annulable). */
  const operation = computed<ServiceOperation | null>(() => tracked.value?.operation ?? null);
  /** L'issue de l'opération, une fois terminée et pas encore acquittée. */
  const finished = computed(() => {
    const id = operationId();
    return id ? operations.finishedOf(serverId(), id) : undefined;
  });

  /** L'agent a accepté l'opération : la fenêtre la suit elle-même. */
  function begin(id: string): void {
    operations.watch(serverId(), id);
  }

  /** L'issue a été dite par la fenêtre : acquittée, jamais rejouée. */
  function settle(): void {
    const id = operationId();
    if (id) operations.settle(serverId(), id);
  }

  /** La fenêtre ne porte plus l'opération : si elle court encore, son issue sera annoncée comme les autres. */
  function release(): void {
    const id = operationId();
    if (id) operations.release(serverId(), id);
  }

  onBeforeUnmount(release);

  return { tracked, operation, finished, begin, settle, release };
}
