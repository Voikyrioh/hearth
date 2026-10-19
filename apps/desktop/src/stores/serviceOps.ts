import { defineStore } from "pinia";
import { ref, shallowReactive } from "vue";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import {
  getLinkBridge,
  isFinished,
  type ServiceOperation,
  type ServiceOperationEvent,
  type Unsubscribe,
} from "@/link";
import { outcomeText } from "@/serviceOps/messages";
import { useServersStore } from "./servers";
import { useServicesStore } from "./services";
import { type ToastKind, useToastsStore } from "./toasts";

/** Nouvelle tentative d'abonnement aux opérations : espacement fixe, tant que l'écran est suivi. */
export const SERVICE_OPS_RETRY_MS = 5000;
/** Garde-fou : les numéros de changement retenus pour écarter un rejeu ne grossissent jamais sans limite. */
const MAX_REMEMBERED = 1000;

/** Une opération suivie, avec le serveur qui la porte et l'ordre dans lequel on l'a vue. */
export interface TrackedOperation {
  serverId: string;
  operation: ServiceOperation;
  /** Ordre d'arrivée (la bande garde les opérations dans cet ordre, la plus ancienne d'abord). */
  order: number;
  /** Le nom du service au moment où l'opération a été vue (le service peut disparaître avec elle). */
  name: string;
}

const OUTCOME_KINDS: Record<string, ToastKind> = {
  succeeded: "success",
  failed: "error",
  cancelled: "warn",
  rolled_back: "warn",
  interrupted: "warn",
};

/**
 * Les opérations de service de chaque serveur (HRT-84, ADR-0045), alimentées par le pont (`onServiceOperations`). La
 * liaison dédoublonne et relit ce que l'agent ne redit pas après une coupure ; ce magasin ne garde que ce qui
 * court. Une opération qui court encore au retour du lien est reconstruite par le pont sans nouveau message :
 * rien ici ne l'annonce. L'issue d'une opération terminée est annoncée UNE fois (une notification), puis
 * acquittée auprès de la coquille, qui ne la rejoue plus (BR-RESIL-010).
 *
 * Aucune règle de statut ici : l'agent les décide ; ce magasin ne choisit que ce qui s'affiche. Un nom de service
 * vient de la liste des services (jamais de l'opération, qui n'en porte pas) et est retenu à la première vue, pour
 * qu'un service supprimé par son opération garde son nom jusqu'à la fin.
 */
export const useServiceOpsStore = defineStore("serviceOps", () => {
  const entries = shallowReactive(new Map<string, TrackedOperation>());
  /** Le dernier numéro de changement vu, par opération : un numéro qui n'est pas plus grand est un rejeu. */
  const lastSeq = new Map<string, number>();
  /** Les opérations dont l'issue a déjà été annoncée : une issue ne se dit qu'une fois (BR-RESIL-010). */
  const announced = new Set<string>();
  /** Le dernier nom vu de chaque service (`serveur/clé`). */
  const names = new Map<string, string>();
  const failed = ref(false);
  const settled = ref(false);
  let unsubscribe: Unsubscribe | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let following = false;
  let pending: Promise<void> | null = null;
  let order = 0;

  const tag = (serverId: string, id: string) => `${serverId}/${id}`;

  /** Les opérations qui courent sur un serveur, la plus ancienne d'abord. */
  function of(serverId: string): TrackedOperation[] {
    return [...entries.values()]
      .filter((tracked) => tracked.serverId === serverId)
      .sort((a, b) => a.order - b.order);
  }

  /** L'opération qui court sur ce service, s'il y en a une (la plus ancienne). */
  function forService(serverId: string, key: string): TrackedOperation | undefined {
    return of(serverId).find((tracked) => tracked.operation.key === key);
  }

  function nameOf(serverId: string, key: string): string {
    const listed = useServicesStore()
      .of(serverId)
      ?.services.find((service) => service.key === key);
    if (listed) {
      names.set(`${serverId}/${key}`, listed.name);
      return listed.name;
    }
    return names.get(`${serverId}/${key}`) ?? t("serviceOps.unknownService");
  }

  function remember(id: string, seq: number): void {
    lastSeq.set(id, seq);
    if (lastSeq.size > MAX_REMEMBERED) {
      const oldest = lastSeq.keys().next().value;
      if (oldest !== undefined) lastSeq.delete(oldest);
    }
  }

  /** Applique un changement de la liaison. Un `seq` qui n'est pas plus grand que le dernier est écarté. */
  function apply(event: ServiceOperationEvent): void {
    if (event.kind === "gone") {
      const id = tag(event.serverId, event.id);
      if (event.seq <= (lastSeq.get(id) ?? 0)) return;
      remember(id, event.seq);
      entries.delete(id);
      return;
    }
    const operation = event.operation;
    const id = tag(event.serverId, operation.id);
    if (event.seq <= (lastSeq.get(id) ?? 0)) return;
    remember(id, event.seq);
    const name = nameOf(event.serverId, operation.key);
    if (isFinished(operation.status)) {
      entries.delete(id);
      // Déjà dite : un rejeu de la coquille (l'acquittement précédent n'est pas arrivé), on la réacquitte.
      if (announced.has(id)) {
        acknowledge(event.serverId, operation.id);
        return;
      }
      announced.add(id);
      if (announced.size > MAX_REMEMBERED) {
        const oldest = announced.values().next().value;
        if (oldest !== undefined) announced.delete(oldest);
      }
      announce(event.serverId, operation, name);
      return;
    }
    // Une opération dont l'issue est déjà dite ne revient pas « en cours » (état en retard).
    if (announced.has(id)) return;
    const known = entries.get(id);
    if (!known) order += 1;
    entries.set(id, {
      serverId: event.serverId,
      operation,
      order: known?.order ?? order,
      name: known?.name ?? name,
    });
  }

  /** L'issue, dite une fois, puis acquittée : la coquille ne la rejoue plus (BR-RESIL-010). */
  function announce(serverId: string, operation: ServiceOperation, name: string): void {
    const message = outcomeText(operation, name);
    if (message) {
      useToastsStore().push({ kind: OUTCOME_KINDS[operation.status] ?? "info", message });
    }
    acknowledge(serverId, operation.id);
  }

  function acknowledge(serverId: string, opId: string): void {
    void getLinkBridge()
      .ackServiceOperations(serverId, [opId])
      .catch((error) => logUiError(error, "serviceOps:ack"));
  }

  async function subscribe(): Promise<void> {
    try {
      const stop = await getLinkBridge().onServiceOperations((event) => apply(event));
      if (!following) {
        stop();
        return;
      }
      unsubscribe = stop;
      failed.value = false;
    } catch (error) {
      if (!following) return;
      failed.value = true;
      logUiError(error, "serviceOps:subscribe");
      timer = setTimeout(() => {
        timer = undefined;
        if (following) pending = subscribe();
      }, SERVICE_OPS_RETRY_MS);
    } finally {
      settled.value = true;
    }
  }

  /**
   * Suit les opérations de tous les serveurs (un seul abonnement, quel que soit le nombre d'appels). Démarré par
   * `App.vue` : une issue se dit quelle que soit la page ouverte. La liste des services est suivie aussi, c'est
   * d'elle que vient le nom d'un service.
   */
  function start(): Promise<void> {
    if (following) return pending ?? Promise.resolve();
    following = true;
    useServicesStore()
      .start()
      .catch((error) => logUiError(error, "serviceOps:services"));
    pending = subscribe();
    return pending;
  }

  /** Cesse de suivre un serveur retiré du carnet et libère ce qu'on en tenait. */
  function forget(serverId: string): void {
    for (const [id, tracked] of [...entries]) if (tracked.serverId === serverId) entries.delete(id);
    for (const id of [...lastSeq.keys()]) if (id.startsWith(`${serverId}/`)) lastSeq.delete(id);
    for (const id of [...names.keys()]) if (id.startsWith(`${serverId}/`)) names.delete(id);
    for (const id of [...announced]) if (id.startsWith(`${serverId}/`)) announced.delete(id);
  }

  // Un serveur retiré du carnet est oublié.
  const servers = useServersStore();
  servers.$subscribe(() => {
    if (!servers.loaded) return;
    const ids = servers.servers.map((server) => server.id);
    for (const tracked of [...entries.values()]) {
      if (!ids.includes(tracked.serverId)) forget(tracked.serverId);
    }
  });

  /** Arrête tout (tests). */
  function reset(): void {
    following = false;
    clearTimeout(timer);
    timer = undefined;
    unsubscribe?.();
    unsubscribe = null;
    pending = null;
    entries.clear();
    lastSeq.clear();
    announced.clear();
    names.clear();
    failed.value = false;
    settled.value = false;
    order = 0;
  }

  return { entries, failed, settled, of, forService, nameOf, apply, start, forget, reset };
});
