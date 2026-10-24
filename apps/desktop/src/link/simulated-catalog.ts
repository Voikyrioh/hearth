import { nameIssue } from "@/deploy/rules";
import type { Catalog, FicheDetail, FicheInstallRequest, FicheRead } from "./catalog";
import type { DeployOutcome, DeployRefusal, DeployStep } from "./deploy";
import type { SimulatedDeploys } from "./simulated-deploy";
import { rich, terraria } from "./simulated-fiches";
import { judgeValue } from "./simulated-judge";

/**
 * Le Catalogue et l'installation depuis une fiche du pont SIMULÉ (navigateur de développement, Vitest, Playwright) :
 * un agent d'essai qui juge une installation COMME LE VRAI (`crates/hearth-agent/src/application/fiche_install.rs`,
 * `domain/services/from_fiche.rs`) : même ordre de refus (fiche inconnue, fiche changée, nom, réponses, contrats,
 * ports, limites, machine, place), mêmes codes (`FICHE_UNKNOWN`, `FICHE_CHANGED`, `AGREEMENT_REQUIRED`,
 * `SETTING_REFUSED`, `HOST_TOO_SMALL`, `NO_SPACE`), puis le déploiement ordinaire (`SimulatedDeploys.admit`). Un test
 * qui passe ici passerait contre l'agent ; le simulateur n'est pas plus indulgent que lui. Les règles de valeur sont
 * écrites ICI, à part de celles du client (`fiche/answers.ts`) : si le client se trompe, ce juge le dit.
 *
 * Les règles de valeur, de variante et de port sont écrites ICI à part, À LA MAIN, et volontairement : ce simulateur
 * est le juge indépendant des règles du client (`fiche/answers.ts`, `fiche/ports.ts`, `fiche/fit.ts`). S'il les importait,
 * une erreur du client serait aussi une erreur du juge et aucun test ne la verrait.
 *
 * La fiche simulée est Terraria, dans la forme réelle de `GET /catalog/{id}` ; `addRich()` ajoute une fiche d'essai
 * qui porte tout ce qu'une fiche peut offrir. Jamais livré (derrière `import.meta.env.DEV`).
 */

/** Une demande reçue, telle que le serveur la garde pour un test : jamais une valeur dans un état de l'interface. */
export interface ReceivedInstall {
  serverId: string;
  request: FicheInstallRequest;
}

/** Le plus petit port d'une fiche (`BR-DEPLOY-007` : jamais sous 1024). */
const MIN_FICHE_PORT = 1024;

export class SimulatedCatalog {
  /** Les installations ACCEPTÉES, dans l'ordre. */
  readonly received: ReceivedInstall[] = [];
  /** Les fiches écartées au chargement (dossier et invariant). */
  refused: { dir: string; invariant: string }[] = [];
  /** Les fiches d'une version plus récente que le client. */
  unreadable = 0;
  /** `true` : la lecture du catalogue échoue (agent qui ne répond pas). */
  failRead = false;
  /** Appelé quand une installation est ACCEPTÉE : le service de jeu a désormais ses réglages (HRT-109). */
  onInstalled:
    | ((
        serverId: string,
        key: string,
        detail: FicheDetail,
        answers: readonly { id: string; value: string }[],
      ) => void)
    | null = null;
  private readonly fiches = new Map<string, FicheDetail>();

  constructor(private readonly deploys: SimulatedDeploys) {
    this.fiches.set("terraria", terraria());
  }

  // ── Ce que l'agent propose (pilotable) ───────────────────────────────────────────────────────

  /** Ajoute (ou remplace) une fiche. */
  put(detail: FicheDetail): void {
    this.fiches.set(detail.card.id, structuredClone(detail));
  }

  /** La fiche d'essai riche : variantes, contrat, port facultatif, ressources mesurées, jeton de serveur. */
  addRich(): FicheDetail {
    const detail = rich();
    this.put(detail);
    return detail;
  }

  remove(id: string): void {
    this.fiches.delete(id);
  }

  /** Une fiche telle que l'agent la garde (de quoi la modifier dans un test). */
  get(id: string): FicheDetail | undefined {
    return this.fiches.get(id);
  }

  /** L'agent remplace une fiche par une version plus récente : version et empreinte changent. */
  bump(id: string): void {
    const detail = this.fiches.get(id);
    if (!detail) return;
    detail.card.version += 1;
    detail.digest = `${detail.digest.slice(8)}${detail.digest.slice(0, 8)}`;
  }

  // ── Les lectures ─────────────────────────────────────────────────────────────────────────────

  list(): Catalog {
    if (this.failRead) throw new Error("catalogue illisible");
    return {
      fiches: [...this.fiches.values()].map((detail) => ({ ...detail.card })),
      refused: this.refused.map((entry) => ({ ...entry })),
      unreadable: this.unreadable,
    };
  }

  read(id: string): FicheRead {
    if (this.failRead) throw new Error("fiche illisible");
    const detail = this.fiches.get(id);
    return detail ? { kind: "found", fiche: structuredClone(detail) } : { kind: "unknown" };
  }

  // ── L'installation (`POST /deployments` avec un membre `source`) ─────────────────────────────

  /**
   * Juge et accepte une installation, ou la refuse. Le droit, la confirmation et le moteur ont déjà été jugés par
   * l'appelant. Les contrôles dans l'ordre de l'agent : fiche proposée et inchangée, nom, contrats, réponses,
   * ports, limites obligatoires, machine, espace, puis le déploiement ordinaire.
   */
  install(serverId: string, request: FicheInstallRequest, by: string): DeployOutcome {
    const refused = (refusal: DeployRefusal): DeployOutcome => ({ kind: "refused", refusal });
    const detail = this.fiches.get(request.fiche);
    if (!detail) return refused({ kind: "fiche_unknown" });
    if (detail.card.version !== request.version || detail.digest !== request.digest) {
      return refused({ kind: "fiche_changed" });
    }
    if (!detail.card.installable) {
      // L'agent répond sur le champ `source`, que la coquille ne sait pas nommer : « autre refus ».
      return refused({ kind: "other" });
    }
    if (nameIssue(request.name) !== null) {
      return refused({ kind: "invalid", field: "name", reason: "bad_format" });
    }
    if (request.variant !== null && !detail.variants.some((item) => item.id === request.variant)) {
      return refused({ kind: "other" });
    }

    // Les contrats : l'agent refuse même d'un client modifié.
    const missing = detail.agreements
      .filter((agreement) => !request.agreements.includes(agreement.id))
      .map((agreement) => agreement.id);
    if (missing.length > 0) return refused({ kind: "agreement_required", ids: missing });

    // Les réponses : une par question, jamais deux ; une question hors variante est refusée.
    const seen = new Set<string>();
    for (const answer of request.answers) {
      if (seen.has(answer.id)) return refused({ kind: "other" });
      seen.add(answer.id);
      // Toute réponse est un réglage de la fiche, essentiel ou non (« Voir tous les réglages », BR-FICHE-040) ; un
      // réglage imposé ne se répond pas. Une fiche qui n'a pas de catalogue complet (agent d'avant) n'a que ses questions.
      const setting = detail.settings.find((item) => item.question.id === answer.id);
      const question = detail.questions.find((item) => item.id === answer.id) ?? setting?.question;
      if (!question) {
        return refused({ kind: "answer_refused", id: null, reason: "unknown_id", character: null });
      }
      if (setting?.imposedWhy) {
        return refused({
          kind: "answer_refused",
          id: question.id,
          reason: "imposed",
          character: null,
        });
      }
      if (
        question.variants.length > 0 &&
        (request.variant === null || !question.variants.includes(request.variant))
      ) {
        return refused({
          kind: "answer_refused",
          id: question.id,
          reason: "variant",
          character: null,
        });
      }
      const judged = judgeValue(question, answer.value, setting?.list ?? null);
      if (judged) {
        return refused({
          kind: "answer_refused",
          id: question.id,
          reason: judged.reason,
          character: judged.character,
        });
      }
    }

    // Les ports : tout port publié toujours a son port du serveur, au-dessus de 1024.
    const hostPorts = new Map<string, number>();
    for (const choice of request.ports) {
      if (hostPorts.has(choice.id)) return refused({ kind: "other" });
      hostPorts.set(choice.id, choice.hostPort);
    }
    const published = detail.ports.filter(
      (port) =>
        (port.onlyVariants.length === 0 ||
          (request.variant !== null && port.onlyVariants.includes(request.variant))) &&
        (port.publish === "always" ||
          (port.publish === "optional" && request.optionalPorts.includes(port.id))),
    );
    for (const port of published) {
      const host = hostPorts.get(port.id);
      if (host === undefined || host < MIN_FICHE_PORT || host > 65535) {
        return refused({ kind: "other" });
      }
    }
    if ([...hostPorts.keys()].some((id) => !published.some((port) => port.id === id))) {
      return refused({ kind: "other" });
    }

    // Les limites sont obligatoires, puis la machine et l'espace : seul un minimum MESURÉ est opposable.
    const limits = this.deploys.limitsRefusal(serverId, request.resources);
    if (limits) return refused(limits);
    const capacity = this.deploys.capacity(serverId);
    const isMeasured = (evidence: { kind: string }) =>
      evidence.kind === "measured" || evidence.kind === "measured_idle";
    const cpu = detail.resources.cpu;
    if (capacity && cpu.minMillis !== null && isMeasured(cpu.evidence)) {
      const has = capacity.cpus * 1000;
      if (cpu.minMillis > has) {
        return refused({ kind: "host_too_small", lack: "cpu", needed: cpu.minMillis, has });
      }
    }
    const memory = detail.resources.memory;
    if (
      capacity &&
      memory.minBytes !== null &&
      isMeasured(memory.evidence) &&
      memory.minBytes > capacity.memoryBytes
    ) {
      return refused({
        kind: "host_too_small",
        lack: "memory",
        needed: memory.minBytes,
        has: capacity.memoryBytes,
      });
    }
    const disk = detail.resources.disk;
    const free = this.deploys.freeDiskBytes(serverId);
    if (disk.installBytes !== null && isMeasured(disk.evidence) && disk.installBytes > free) {
      return refused({
        kind: "no_space_to_install",
        neededBytes: disk.installBytes,
        freeBytes: free,
        missingBytes: disk.installBytes - free,
      });
    }
    if (
      disk.installBytes !== null &&
      disk.evidence.kind === "publisher" &&
      disk.installBytes > free &&
      !request.accept.lowDisk
    ) {
      return refused({ kind: "no_space", freeBytes: free });
    }

    const outcome = this.deploys.admit(
      serverId,
      {
        name: request.name,
        repository: detail.image.repository,
        tag: detail.image.tag,
        ports: published.map((port) => ({
          hostPort: hostPorts.get(port.id) ?? 0,
          // `same_number` : le jeu annonce son port, le service prend le même nombre que le serveur.
          containerPort:
            port.mapping === "same_number" ? (hostPorts.get(port.id) ?? 0) : port.container,
          protocol: port.protocol,
        })),
        accept: request.accept,
      },
      by,
      () => this.received.push({ serverId, request: structuredClone(request) }),
    );
    if (outcome.kind === "accepted") {
      this.onInstalled?.(serverId, outcome.serviceKey, detail, request.answers);
    }
    return outcome;
  }

  /** Les déploiements en cours d'un serveur, de quoi les piloter (même suite qu'un déploiement d'image). */
  running(
    serverId: string,
  ): { operationId: string; key: string; name: string; step: DeployStep }[] {
    return this.deploys.running(serverId);
  }
}
