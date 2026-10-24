import { nameIssue } from "@/deploy/rules";
import type {
  Catalog,
  FicheDetail,
  FicheInstallRequest,
  FicheQuestion,
  FicheRead,
} from "./catalog";
import type { DeployOutcome, DeployRefusal, DeployStep } from "./deploy";
import type { SimulatedDeploys } from "./simulated-deploy";
import { rich, terraria } from "./simulated-fiches";

/**
 * Le Catalogue et l'installation depuis une fiche du pont SIMULÉ (navigateur de développement, Vitest, Playwright) :
 * un agent d'essai qui juge une installation COMME LE VRAI (`crates/hearth-agent/src/application/fiche_install.rs`,
 * `domain/services/from_fiche.rs`) : même ordre de refus (fiche inconnue, fiche changée, nom, réponses, contrats,
 * ports, limites, machine, place), mêmes codes (`FICHE_UNKNOWN`, `FICHE_CHANGED`, `AGREEMENT_REQUIRED`,
 * `SETTING_REFUSED`, `HOST_TOO_SMALL`, `NO_SPACE`), puis le déploiement ordinaire (`SimulatedDeploys.admit`). Un test
 * qui passe ici passerait contre l'agent ; le simulateur n'est pas plus indulgent que lui. Les règles de valeur sont
 * écrites ICI, à part de celles du client (`fiche/answers.ts`) : si le client se trompe, ce juge le dit.
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

  /** Une valeur jugée comme le fait l'agent (`validate_value`) : la raison et, au plus, UN caractère refusé. */
  private judgeValue(
    question: FicheQuestion,
    value: string,
  ): { reason: string; character: string | null } | null {
    const refuse = (reason: string, character: string | null = null) => ({ reason, character });
    const length = [...value].length;
    const lengthBad =
      (question.minLen !== null && length < question.minLen) ||
      (question.maxLen !== null && length > question.maxLen);
    const risky = (char: string) =>
      /[\p{Cc}\u2028\u2029\u200B-\u200F\u202A-\u202E\u2066-\u2069]/u.test(char);
    switch (question.kind) {
      case "text": {
        if (lengthBad) return refuse("length");
        for (const char of value) {
          const bad =
            question.charset === "file_stem"
              ? !/^[A-Za-z0-9_-]$/.test(char)
              : question.charset === "arg_safe"
                ? !/^[A-Za-z0-9._:-]$/.test(char)
                : risky(char);
          if (bad) return refuse("charset", char);
        }
        if (question.charset === "arg_safe" && /^[-+]/.test(value)) {
          return refuse("charset", value[0] ?? null);
        }
        return null;
      }
      case "secret": {
        if (lengthBad) return refuse("length");
        return [...value].some(risky) ? refuse("charset") : null;
      }
      case "int": {
        const number = Number(value);
        if (!/^-?\d+$/.test(value) || !Number.isSafeInteger(number) || String(number) !== value) {
          return refuse("bounds");
        }
        const below = question.min !== null && number < question.min;
        const above = question.max !== null && number > question.max;
        return below || above ? refuse("bounds") : null;
      }
      case "bool":
      case "choice":
        return question.choices.some((choice) => choice.value === value) ? null : refuse("choice");
      default:
        return lengthBad ? refuse("length") : null;
    }
  }

  /**
   * Juge et accepte une installation, ou la refuse. Le droit, la confirmation et le moteur ont déjà été jugés par
   * l'appelant. Les contrôles dans l'ordre de l'agent : fiche proposée et inchangée, nom, réponses, contrats,
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
      return refused({ kind: "invalid", field: "name", reason: "platform_unavailable" });
    }
    if (nameIssue(request.name) !== null) {
      return refused({ kind: "invalid", field: "name", reason: "bad_format" });
    }
    if (request.variant !== null && !detail.variants.some((item) => item.id === request.variant)) {
      return refused({ kind: "other" });
    }

    // Les réponses : une par question, jamais deux ; une question hors variante est refusée.
    const seen = new Set<string>();
    for (const answer of request.answers) {
      if (seen.has(answer.id)) return refused({ kind: "other" });
      seen.add(answer.id);
      const question = detail.questions.find((item) => item.id === answer.id);
      if (!question) {
        return refused({ kind: "answer_refused", id: null, reason: "unknown_id", character: null });
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
      const judged = this.judgeValue(question, answer.value);
      if (judged) {
        return refused({
          kind: "answer_refused",
          id: question.id,
          reason: judged.reason,
          character: judged.character,
        });
      }
    }

    // Les contrats : l'agent refuse même d'un client modifié.
    const missing = detail.agreements
      .filter((agreement) => !request.agreements.includes(agreement.id))
      .map((agreement) => agreement.id);
    if (missing.length > 0) return refused({ kind: "agreement_required", ids: missing });

    // Les ports : tout port publié toujours a son port du serveur, au-dessus de 1024.
    const hostPorts = new Map<string, number>();
    for (const choice of request.ports) {
      if (hostPorts.has(choice.id)) return refused({ kind: "other" });
      hostPorts.set(choice.id, choice.hostPort);
    }
    const published = detail.ports.filter(
      (port) =>
        port.publish === "always" ||
        (port.publish === "optional" && request.optionalPorts.includes(port.id)),
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
    if (capacity && cpu.minMillis !== null && cpu.evidence.kind === "measured") {
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

    return this.deploys.admit(
      serverId,
      {
        name: request.name,
        repository: detail.image.repository,
        tag: detail.image.tag,
        ports: published.map((port) => ({
          hostPort: hostPorts.get(port.id) ?? 0,
          containerPort: port.container,
          protocol: port.protocol,
        })),
        accept: request.accept,
      },
      by,
      () => this.received.push({ serverId, request: structuredClone(request) }),
    );
  }

  /** Les déploiements en cours d'un serveur, de quoi les piloter (même suite qu'un déploiement d'image). */
  running(
    serverId: string,
  ): { operationId: string; key: string; name: string; step: DeployStep }[] {
    return this.deploys.running(serverId);
  }
}
