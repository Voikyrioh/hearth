import type {
  ServicesEvent as BoundServicesEvent,
  EngineDto,
  ServiceDto,
  ServicesViewDto,
} from "@/bindings";
import { type EngineInfo, type ServicesEvent, toEngine, toServicesEvent } from "./services";
import { LinkCommandError, type Unsubscribe } from "./types";

/**
 * Le moteur de conteneurs et les services du pont SIMULÉ (navigateur de développement, Vitest, Playwright) :
 * un petit agent que l'appelant PILOTE. Il produit les MÊMES formes que la coquille : des événements du type
 * généré `ServicesEvent` (`bindings.ts`), donc `tsc` refuse tout champ en trop, manquant ou mal typé ; la
 * fiche `src-tauri/tests/fixtures/services-event.json`, que la coquille compare à `hearth-proto` champ par
 * champ, est relue par les tests de l'interface pour contrôler que rien ne manque ici (voir
 * `simulated-services.test.ts`). Comme `hearth-link`, il tient l'état COMPLET trié par nom sans tenir compte de
 * la casse et n'annonce que des états complets ; comme un agent, il ne dit rien tant que le lien n'est pas
 * « Connecté » et renvoie l'état courant à son retour. Jamais livré (derrière `import.meta.env.DEV`). Aucune
 * valeur ni nom de variable d'environnement, aucune commande.
 */

interface Book {
  seq: number;
  view: ServicesViewDto;
}

export type PartialEngine = Partial<Omit<EngineDto, "host">> & {
  host?: Partial<EngineDto["host"]>;
};
export type PartialService = Partial<Omit<ServiceDto, "image">> & {
  key: string;
  name: string;
  image?: Partial<ServiceDto["image"]>;
};

/** Ordre de la liste (`BR-SVC-012`) : le nom sans tenir compte de la casse, la clé départage. */
export function byName(a: ServiceDto, b: ServiceDto): number {
  const left = a.name.toLowerCase();
  const right = b.name.toLowerCase();
  if (left !== right) return left < right ? -1 : 1;
  return a.key < b.key ? -1 : a.key > b.key ? 1 : 0;
}

export class SimulatedServices {
  private readonly books = new Map<string, Book>();
  private readonly listeners = new Set<(event: ServicesEvent) => void>();
  /** `true` : l'abonnement échoue (la lecture de la liste est en échec). */
  failSubscribe = false;
  /** `true` : « Vérifier à nouveau » échoue. */
  failRecheck = false;
  /** Chaque « Vérifier à nouveau » demandé, dans l'ordre. */
  readonly rechecks: string[] = [];
  /** Combien de fois la documentation de Docker a été ouverte. */
  docsOpened = 0;
  /** Combien d'abonnements ont été posés (les tests s'en servent pour « reprendre la lecture »). */
  subscriptions = 0;

  constructor(
    private readonly now: () => number,
    private readonly connected: (serverId: string) => boolean,
  ) {}

  // ── Fabriques : les valeurs par défaut d'un agent, à compléter par le test ───────────────────

  engine(patch: PartialEngine = {}): EngineDto {
    const at = new Date(this.now()).toISOString();
    return {
      state: "ready",
      name: "Docker Engine - Community",
      version: "24.0.7",
      minVersion: "20.10",
      agentAccount: "root",
      agentIsAdmin: true,
      lastReadyAt: at,
      checkedAt: at,
      listReadAt: at,
      listDegraded: false,
      at,
      listCurrentAt: at,
      ...patch,
      host: { osId: "debian", osLike: [], init: "systemd", declarative: false, ...patch.host },
    };
  }

  service(patch: PartialService): ServiceDto {
    return {
      origin: "external",
      driver: null,
      unit: null,
      removedOnStop: false,
      composeProject: null,
      state: "running",
      since: new Date(this.now() - 3_600_000).toISOString(),
      createdAt: new Date(this.now() - 86_400_000).toISOString(),
      neverStarted: false,
      exitCode: null,
      restarts10m: 0,
      ports: [],
      portsOmitted: 0,
      hostNetwork: false,
      ...patch,
      image: { ref: "nginx", tag: "1.27", shortId: "0123456789ab", ...patch.image },
    };
  }

  // ── Pilotage ─────────────────────────────────────────────────────────────────────────────────

  /** Remplace tout d'un serveur : le moteur (prêt par défaut) et sa liste. */
  seed(serverId: string, services: PartialService[] = [], engine: PartialEngine = {}): void {
    this.store(serverId, {
      kind: "supported",
      engine: this.engine(engine),
      services: services.map((service) => this.service(service)).sort(byName),
    });
  }

  /** `n` services d'exemple, noms numérotés (liste de 200 services, défilement, recherche). */
  generate(serverId: string, count: number, engine: PartialEngine = {}): void {
    const services: PartialService[] = [];
    const states: ServiceDto["state"][] = ["running", "running", "running", "stopped", "error"];
    for (let index = 0; index < count; index += 1) {
      const state = states[index % states.length] ?? "running";
      services.push({
        key: `c_${String(index).padStart(4, "0")}`,
        name: `service-${String(index).padStart(3, "0")}`,
        state,
        exitCode: state === "error" ? 137 : null,
        image: { ref: `registre/image-${index % 17}`, tag: `1.${index % 9}` },
        ports:
          index % 3 === 0
            ? [{ hostPort: 8000 + index, containerPort: 80, protocol: "tcp", scope: "all" }]
            : [],
      });
    }
    this.seed(serverId, services, engine);
  }

  /** Change le moteur (état, nom, version…), la liste reste. */
  setEngine(serverId: string, patch: PartialEngine): void {
    const book = this.books.get(serverId);
    const current: ServicesViewDto =
      book && book.view.kind === "supported"
        ? book.view
        : { kind: "supported", engine: this.engine(), services: [] };
    if (current.kind !== "supported") return;
    this.store(serverId, {
      kind: "supported",
      engine: {
        ...current.engine,
        // Une vue refabriquée porte l'heure de l'agent à cet instant (HRT-72).
        at: new Date(this.now()).toISOString(),
        ...patch,
        host: { ...current.engine.host, ...patch.host },
      },
      services: current.services,
    });
  }

  /** Crée ou modifie un service (par sa clé) : il apparaît à sa place alphabétique. */
  upsert(serverId: string, patch: PartialService): void {
    this.upsertQuiet(serverId, patch);
    this.announce(serverId);
  }

  /** Change plusieurs services d'un coup, un seul état annoncé. */
  upsertMany(serverId: string, patches: PartialService[]): void {
    for (const patch of patches) this.upsertQuiet(serverId, patch);
    this.announce(serverId);
  }

  remove(serverId: string, key: string): void {
    const book = this.books.get(serverId);
    if (book?.view.kind !== "supported") return;
    this.store(serverId, {
      kind: "supported",
      engine: book.view.engine,
      services: book.view.services.filter((item) => item.key !== key),
    });
  }

  /** L'agent de ce serveur est « d'avant les services » : pas de capacité dans son `/hello`. */
  setUnsupported(serverId: string, agentVersion: string): void {
    this.store(serverId, { kind: "unsupported", agentVersion });
  }

  /** Efface tout ce que l'agent a dit de ce serveur (retour à « première détection »). */
  forget(serverId: string): void {
    this.books.delete(serverId);
  }

  /** Le dernier événement d'un serveur, tel que la coquille le rejouerait. */
  current(serverId: string): BoundServicesEvent | undefined {
    const book = this.books.get(serverId);
    return book ? { serverId, seq: book.seq, view: book.view } : undefined;
  }

  // ── Ce que voit le pont ───────────────────────────────────────────────────────────────────────

  async subscribe(listener: (event: ServicesEvent) => void): Promise<Unsubscribe> {
    this.subscriptions += 1;
    if (this.failSubscribe) throw new LinkCommandError({ kind: "internal" });
    this.listeners.add(listener);
    for (const serverId of this.books.keys()) {
      const event = this.current(serverId);
      if (event) listener(toServicesEvent(event));
    }
    return () => void this.listeners.delete(listener);
  }

  /** « Vérifier à nouveau » : la détection est relancée, le moteur répond (l'état publié par le flux suit). */
  async recheck(serverId: string): Promise<EngineInfo> {
    if (!this.connected(serverId)) throw new LinkCommandError({ kind: "not_connected" });
    this.rechecks.push(serverId);
    if (this.failRecheck) throw new LinkCommandError({ kind: "unreachable" });
    const book = this.books.get(serverId);
    const engine = book && book.view.kind === "supported" ? book.view.engine : this.engine();
    return toEngine(engine);
  }

  /** Le lien revient : l'agent renvoie l'état courant à l'abonné (un nouvel instantané). */
  resync(serverId: string): void {
    this.announce(serverId);
  }

  // ── Interne ───────────────────────────────────────────────────────────────────────────────────

  private upsertQuiet(serverId: string, patch: PartialService): void {
    const book = this.books.get(serverId);
    const view: ServicesViewDto =
      book && book.view.kind === "supported"
        ? book.view
        : { kind: "supported", engine: this.engine(), services: [] };
    if (view.kind !== "supported") return;
    const existing = view.services.find((item) => item.key === patch.key);
    const merged: ServiceDto = existing
      ? { ...existing, ...patch, image: { ...existing.image, ...patch.image } }
      : this.service(patch);
    this.books.set(serverId, {
      seq: book?.seq ?? 0,
      view: {
        kind: "supported",
        engine: view.engine,
        services: [...view.services.filter((item) => item.key !== patch.key), merged].sort(byName),
      },
    });
  }

  private store(serverId: string, view: ServicesViewDto): void {
    const seq = (this.books.get(serverId)?.seq ?? 0) + 1;
    this.books.set(serverId, { seq, view });
    this.emit(serverId);
  }

  private announce(serverId: string): void {
    const book = this.books.get(serverId);
    if (!book) return;
    this.books.set(serverId, { seq: book.seq + 1, view: book.view });
    this.emit(serverId);
  }

  private emit(serverId: string): void {
    if (!this.connected(serverId)) return;
    const event = this.current(serverId);
    if (!event) return;
    const converted = toServicesEvent(event);
    for (const listener of [...this.listeners]) listener(converted);
  }
}
