import type { ServiceDto } from "@/bindings";
import type { CancelOutcome } from "./deploy";
import type {
  DeleteOutcome,
  DeletePreview,
  DeletePreviewOutcome,
  DeleteRefusal,
  DeleteRequest,
  FolderPreview,
} from "./serviceDelete";
import { toOperation } from "./serviceOps";
import type { SimulatedDeploys } from "./simulated-deploy";
import type { SimulatedServiceOps } from "./simulated-service-ops";
import type { SimulatedServices } from "./simulated-services";

/**
 * La suppression d'un service du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * juge une demande COMME LE VRAI (`crates/hearth-agent/src/application/delete.rs`) : service inconnu ou adopté, action en
 * cours, état qui ne se supprime pas, case des sauvegardes refusée (l'agent n'en a pas), nom retapé quand la case des
 * données est cochée. La consultation (`preview`) rend ce que l'appelant a posé (`setPreview`), sinon un dossier de
 * dossier géré par montage de dossier du service (le détail et la fenêtre disent la même chose). La suite de l'opération (étapes, effacement, issue) est PILOTÉE
 * par l'appelant : `toStep`, `erasing`, `succeed`, `fail` ; rien n'avance seul. Jamais livré (derrière
 * `import.meta.env.DEV`).
 */

const GIB = 1024 ** 3;

/** Les étapes du plan d'une suppression, dans l'ordre (`domain/ops/delete.rs`). */
export const SIM_DELETE_STEPS = ["stop", "remove_container", "forget", "erase"] as const;
export type SimDeleteStep = (typeof SIM_DELETE_STEPS)[number];

/** Les états où une suppression est possible (`deletable` de l'agent, BR-DEL-003). */
const DELETABLE: readonly ServiceDto["state"][] = ["running", "stopped", "error", "restart_loop"];

interface Held {
  operationId: string;
  key: string;
  name: string;
  request: DeleteRequest;
  /** Le service tournait : l'arrêt est une étape du plan. */
  running: boolean;
}

export class SimulatedDeletes {
  private readonly previews = new Map<string, Partial<DeletePreview>>();
  private readonly held = new Map<string, Map<string, Held>>();
  /** Les consultations reçues, dans l'ordre. */
  readonly previewed: { serverId: string; key: string }[] = [];
  /** Les demandes d'effacement reçues, dans l'ordre (jamais de mot de passe). */
  readonly received: { serverId: string; key: string; request: DeleteRequest }[] = [];
  /** Les annulations reçues. */
  readonly cancels: { serverId: string; opId: string }[] = [];
  /** `true` : la consultation échoue (lien sans réponse), pour un test. */
  failPreview = false;
  /** Retard de la consultation (ms) : la mesure des dossiers dure (BR-DEL-009). */
  previewDelayMs = 0;

  constructor(
    private readonly services: SimulatedServices,
    private readonly ops: SimulatedServiceOps,
    private readonly deploys: SimulatedDeploys,
  ) {}

  /** Ce que la consultation de ce service rendra (le reste a des valeurs de départ). */
  setPreview(serverId: string, key: string, preview: Partial<DeletePreview>): void {
    this.previews.set(`${serverId}/${key}`, preview);
  }

  /** Un dossier géré par service monté (le détail et la fenêtre disent la même chose) : ses dossiers sont ses montages. */
  private defaultFolders(found: ServiceDto): FolderPreview[] {
    return found.mounts
      .filter((mount) => mount.kind === "folder")
      .map((mount) => ({
        path: mount.source,
        bytes: 1.2 * GIB,
        items: 48_210,
        partial: false,
        unmeasured: false,
        fate: "erasable" as const,
        sharedWith: [],
        sharedWithUnmanaged: false,
      }));
  }

  /** `GET /services/{key}/delete-preview` : un service créé par Hearth, ou un refus lu par son code. */
  preview(serverId: string, key: string): DeletePreviewOutcome {
    this.previewed.push({ serverId, key });
    const engine = this.services.engineState(serverId);
    // Comme l'agent : le moteur d'abord (BR-DEL-022), puis le service.
    if (engine !== "ready") {
      return { kind: "refused", refusal: { kind: "engine_unavailable", engine } };
    }
    const found = this.services.find(serverId, key);
    if (!found || (found.origin !== "hearth" && found.adopted === null)) {
      return { kind: "refused", refusal: { kind: "service_not_found" } };
    }
    if (found.adopted !== null) return { kind: "refused", refusal: { kind: "service_adopted" } };
    const posed = this.previews.get(`${serverId}/${key}`) ?? {};
    const running = found.state === "running" || found.state === "restart_loop";
    return {
      kind: "ready",
      preview: {
        name: found.name,
        image: `${found.image.ref}:${found.image.tag ?? "latest"}`,
        running,
        stopGraceS: found.graceS ?? 10,
        containerMissing: false,
        folders: this.defaultFolders(found),
        serverFolders: [],
        volumes: 0,
        measuredInTime: true,
        backups: 0,
        ...posed,
      },
    };
  }

  /**
   * Juge et accepte une demande, ou la refuse. Les contrôles dans l'ordre de l'agent : service, action en cours, état,
   * sauvegardes, nom retapé. Le droit, la confirmation et le moteur ont déjà été jugés par l'appelant.
   */
  accept(serverId: string, key: string, request: DeleteRequest, by: string): DeleteOutcome {
    const found = this.services.find(serverId, key);
    const refuse = (refusal: DeleteRefusal): DeleteOutcome => ({ kind: "refused", refusal });
    if (!found || (found.origin !== "hearth" && found.adopted === null)) {
      return refuse({ kind: "service_not_found" });
    }
    if (found.adopted !== null) return refuse({ kind: "service_adopted" });
    if (found.busy) {
      return refuse({
        kind: "service_busy",
        by: found.busy.by,
        action: found.busy.kind as never,
      });
    }
    if (!DELETABLE.includes(found.state))
      return refuse({ kind: "not_drivable", state: found.state });
    if (request.eraseBackups) return refuse({ kind: "backups_unavailable" });
    if (request.eraseData && request.confirmName !== found.name) {
      return refuse({ kind: "name_mismatch" });
    }
    this.received.push({ serverId, key, request: structuredClone(request) });
    const running = found.state === "running" || found.state === "restart_loop";
    const operation = this.ops.accept(
      serverId,
      {
        key,
        kind: "delete",
        // Acceptée, pas encore exécutée : rien n'est fait, l'annulation est possible (BR-DEL-014).
        step: null,
        steps: [],
        cancellable: true,
      },
      by,
    );
    const mine = this.held.get(serverId) ?? new Map<string, Held>();
    mine.set(operation.id, { operationId: operation.id, key, name: found.name, request, running });
    this.held.set(serverId, mine);
    // Le nom est réservé pendant toute l'opération (BR-DEL-019).
    this.deploys.markDeleting(serverId, found.name);
    return { kind: "accepted", operationId: operation.id };
  }

  // ── La suite de l'opération, pilotée par l'appelant ──────────────────────────────────────────

  private heldOf(serverId: string, opId: string): Held {
    const found = this.held.get(serverId)?.get(opId);
    if (!found) throw new Error(`suppression inconnue : ${opId}`);
    return found;
  }

  /** Les lignes du journal d'un plan arrivé à `step` (« remove_container » est le point de non-retour). */
  private stepsUpTo(held: Held, step: SimDeleteStep, last: "intent" | "done" = "intent") {
    const wanted = SIM_DELETE_STEPS.filter((name) => {
      if (name === "stop") return held.running;
      if (name === "erase") return held.request.eraseData;
      return true;
    });
    const index = wanted.indexOf(step);
    return wanted.slice(0, index + 1).map((name, i) => ({
      step: name as string,
      state: i < index ? ("done" as const) : last,
    }));
  }

  /** Passe à une étape (« remove_container » est le point de non-retour : l'annulation devient impossible). */
  toStep(serverId: string, opId: string, step: SimDeleteStep): void {
    const held = this.heldOf(serverId, opId);
    if (step === "remove_container" || step === "forget" || step === "erase") {
      // Le conteneur est supprimé, la description oubliée : le service sort de la liste.
      this.services.remove(serverId, held.key);
    }
    this.ops.update(serverId, opId, {
      status: "running",
      step,
      steps: this.stepsUpTo(held, step),
      // « remove_container » est le point de non-retour : dès qu'il commence, le conteneur part (BR-DEL-014).
      cancellable: step === "stop",
    });
  }

  /** L'effacement progresse : `itemsDone` éléments effacés (BR-DEL-030). */
  erasing(serverId: string, opId: string, itemsDone: number): void {
    const held = this.heldOf(serverId, opId);
    this.services.remove(serverId, held.key);
    this.ops.update(serverId, opId, {
      status: "running",
      step: "erase",
      steps: this.stepsUpTo(held, "erase"),
      itemsDone,
      cancellable: false,
    });
  }

  /**
   * La suppression réussit : le service n'est plus. `cause: "data_not_erased"` : le service est supprimé mais des
   * éléments n'ont pas pu être effacés (BR-DEL-016) ; `cause: "forced_stop"` : l'arrêt a été forcé (BR-DEL-013).
   */
  succeed(
    serverId: string,
    opId: string,
    cause: "data_not_erased" | "forced_stop" | null = null,
  ): void {
    const held = this.heldOf(serverId, opId);
    this.services.remove(serverId, held.key);
    this.ops.update(serverId, opId, {
      steps: this.stepsUpTo(held, held.request.eraseData ? "erase" : "forget", "done"),
    });
    this.ops.finish(serverId, opId, "succeeded", cause);
    this.release(serverId, opId, held);
  }

  /** La suppression échoue AVANT le point de non-retour : le service reste entier (BR-DEL-015). */
  fail(
    serverId: string,
    opId: string,
    cause: Parameters<SimulatedServiceOps["finish"]>[3] = "engine_refused",
    status: "failed" | "interrupted" = "failed",
  ): void {
    const held = this.heldOf(serverId, opId);
    this.ops.finish(serverId, opId, status, cause);
    this.release(serverId, opId, held);
  }

  private release(serverId: string, opId: string, held: Held): void {
    this.held.get(serverId)?.delete(opId);
    this.deploys.unmarkDeleting(serverId, held.name);
  }

  /** `DELETE /operations/services/{id}` : annule avant le point de non-retour, sinon `too_late`. */
  cancel(serverId: string, opId: string): CancelOutcome {
    this.cancels.push({ serverId, opId });
    const dto = this.ops.of(serverId).find((item) => item.id === opId);
    if (!dto) return { kind: "refused", refusal: { kind: "not_found" } };
    if (
      dto.status === "cancelling" ||
      (dto.status === "cancelled" && dto.cause === "cancelled_by_account")
    ) {
      return { kind: "done", operation: toOperation(dto) };
    }
    if (["cancelled", "succeeded", "failed", "interrupted"].includes(dto.status)) {
      return { kind: "refused", refusal: { kind: "not_cancellable" } };
    }
    if (!dto.cancellable) return { kind: "refused", refusal: { kind: "too_late" } };
    this.ops.update(serverId, opId, {
      status: "cancelling",
      cancellable: false,
      cause: "cancelled_by_account",
    });
    const held = this.held.get(serverId)?.get(opId);
    setTimeout(() => {
      this.ops.finish(serverId, opId, "cancelled", "cancelled_by_account");
      if (held) this.release(serverId, opId, held);
    }, 0);
    const after = this.ops.of(serverId).find((item) => item.id === opId) ?? dto;
    return { kind: "done", operation: toOperation(after) };
  }

  /** Les suppressions en cours d'un serveur (de quoi vérifier). */
  running(serverId: string): { operationId: string; key: string; name: string }[] {
    return [...(this.held.get(serverId)?.values() ?? [])].map((held) => ({
      operationId: held.operationId,
      key: held.key,
      name: held.name,
    }));
  }
}
