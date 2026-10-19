import type {
  ServicesEvent as BoundServicesEvent,
  EngineDto,
  ServiceDto,
  ServicesViewDto,
} from "@/bindings";
import {
  type EngineInfo,
  type EnvVariable,
  type ServicesEvent,
  toEngine,
  toServicesEvent,
} from "./services";
import { LinkCommandError, type Unsubscribe } from "./types";

/**
 * Le moteur de conteneurs et les services du pont SIMULÉ (navigateur de développement, Vitest, Playwright) :
 * un petit agent que l'appelant PILOTE. Il produit les MÊMES formes que la coquille : des événements du type
 * généré `ServicesEvent` (`bindings.ts`), donc `tsc` refuse tout champ en trop, manquant ou mal typé ; la
 * fiche `src-tauri/tests/fixtures/services-event.json`, que la coquille compare à `hearth-proto` champ par
 * champ, est relue par les tests de l'interface pour contrôler que rien ne manque ici (voir
 * `simulated-services.test.ts`). Comme `hearth-link`, il tient l'état COMPLET trié par nom sans tenir compte de
 * la casse et n'annonce que des états complets ; comme un agent, il ne dit rien tant que le lien n'est pas
 * « Connecté » et renvoie l'état courant à son retour. Jamais livré (derrière `import.meta.env.DEV`). Les
 * noms de variables et la commande suivent la liste (détail, HRT-63) ; les VALEURS ne sortent que par
 * `revealEnv`, qui note chaque lecture (le journal de l'agent) sans jamais la garder.
 */

interface Book {
  seq: number;
  view: ServicesViewDto;
  /** Quand la « liaison » a reçu la vue du moteur (comme `hearth-link`), `null` : inconnue. */
  engineReceivedAt: number | null;
  /** La commande et les noms de variables ont été lus sur le serveur (faux : liste relue du disque). */
  detailsKnown: boolean;
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
  private readonly unreadable = new Map<string, number>();
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
  /** Les valeurs de variables que l'agent simulé connaît, par serveur puis par clé de service (jamais dans un événement). */
  private readonly envValues = new Map<string, EnvVariable[]>();
  /** Chaque révélation réussie, dans l'ordre : ce que l'agent consigne au journal (la clé du service, ni nom ni valeur). */
  readonly reveals: { serverId: string; key: string }[] = [];
  /** `true` : la prochaine lecture des valeurs échoue (lien muet, moteur qui ne répond pas) tant que c'est vrai. */
  failReveal = false;
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
      mounts: [],
      mountsOmitted: 0,
      restart: "unless_stopped",
      envNames: [],
      envNamesOmitted: 0,
      command: null,
      graceS: 10,
      adopted: null,
      detailsReserved: false,
      busy: null,
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
      unreadable: 0,
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

  /**
   * Les variables d'un service : leurs NOMS vont à la liste (comme l'agent), leurs VALEURS restent ici et ne sortent
   * que par `revealEnv`.
   */
  setEnv(serverId: string, key: string, variables: EnvVariable[]): void {
    this.envValues.set(
      `${serverId}/${key}`,
      variables.map((variable) => ({ ...variable })),
    );
    this.upsert(serverId, {
      key,
      name: this.nameOf(serverId, key),
      envNames: variables.map((v) => v.name),
    });
  }

  /** La lecture des valeurs : réservée au droit (vérifié par l'appelant), consignée, jamais rejouée. */
  revealEnv(serverId: string, key: string): EnvVariable[] {
    if (this.failReveal) throw new LinkCommandError({ kind: "unreachable" });
    const book = this.books.get(serverId);
    const exists =
      book?.view.kind === "supported" && book.view.services.some((item) => item.key === key);
    if (!exists) throw new LinkCommandError({ kind: "internal" });
    this.reveals.push({ serverId, key });
    return (this.envValues.get(`${serverId}/${key}`) ?? []).map((variable) => ({ ...variable }));
  }

  private nameOf(serverId: string, key: string): string {
    const book = this.books.get(serverId);
    const found =
      book?.view.kind === "supported"
        ? book.view.services.find((item) => item.key === key)
        : undefined;
    return found?.name ?? key;
  }

  /** Un service de la liste d'un serveur (de quoi vérifier), `undefined` s'il n'y est pas. */
  find(serverId: string, key: string): ServiceDto | undefined {
    const book = this.books.get(serverId);
    return book?.view.kind === "supported"
      ? book.view.services.find((item) => item.key === key)
      : undefined;
  }

  /** L'état du moteur d'un serveur, `null` tant qu'il n'est pas connu. */
  engineState(serverId: string): EngineDto["state"] | null {
    const book = this.books.get(serverId);
    return book?.view.kind === "supported" ? book.view.engine.state : null;
  }

  /** Pose ou retire l'action qui tient un service (`busy`, BR-OPS-005) ; sans effet sur un service inconnu. */
  setBusy(
    serverId: string,
    key: string,
    busy: { kind: string; by: string; since: string } | null,
  ): void {
    const book = this.books.get(serverId);
    if (book?.view.kind !== "supported") return;
    const found = book.view.services.find((item) => item.key === key);
    if (!found) return;
    this.upsert(serverId, { key, name: found.name, busy });
  }

  /** Change le moteur (état, nom, version…), la liste reste. */
  setEngine(serverId: string, patch: PartialEngine): void {
    const book = this.books.get(serverId);
    const current: ServicesViewDto =
      book && book.view.kind === "supported"
        ? book.view
        : { kind: "supported", engine: this.engine(), services: [], unreadable: 0 };
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
      unreadable: current.unreadable,
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
      unreadable: book.view.unreadable,
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

  /**
   * Fixe le repère de réception de la vue du moteur (un état relu du disque ou rejoué bien après sa réception) ;
   * `null` : inconnu, comme pour un fichier écrit avant que ce repère existe.
   */
  setEngineReceivedAt(serverId: string, at: number | null): void {
    const book = this.books.get(serverId);
    if (book) this.books.set(serverId, { ...book, engineReceivedAt: at });
  }

  /**
   * Fait comme une liste relue du disque au redémarrage hors ligne (`false`) : ni commande ni noms de variables
   * (écrits nulle part), et l'événement le dit. `true` : ils ont été lus sur le serveur.
   */
  setDetailsKnown(serverId: string, known: boolean): void {
    const book = this.books.get(serverId);
    if (!book) return;
    const view: ServicesViewDto =
      !known && book.view.kind === "supported"
        ? {
            ...book.view,
            services: book.view.services.map((item) => ({
              ...item,
              envNames: [],
              envNamesOmitted: 0,
              command: null,
            })),
          }
        : book.view;
    this.books.set(serverId, { ...book, view, detailsKnown: known });
    this.announce(serverId);
  }

  /** Le dernier événement d'un serveur, tel que la coquille le rejouerait. */
  current(serverId: string): BoundServicesEvent | undefined {
    const book = this.books.get(serverId);
    return book
      ? {
          serverId,
          seq: book.seq,
          engineReceivedAt: book.engineReceivedAt,
          detailsKnown: book.detailsKnown,
          view: book.view,
        }
      : undefined;
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
        : { kind: "supported", engine: this.engine(), services: [], unreadable: 0 };
    if (view.kind !== "supported") return;
    const existing = view.services.find((item) => item.key === patch.key);
    const merged: ServiceDto = existing
      ? { ...existing, ...patch, image: { ...existing.image, ...patch.image } }
      : this.service(patch);
    this.books.set(serverId, {
      seq: book?.seq ?? 0,
      engineReceivedAt: book ? book.engineReceivedAt : this.now(),
      detailsKnown: book?.detailsKnown ?? true,
      view: {
        kind: "supported",
        engine: view.engine,
        services: [...view.services.filter((item) => item.key !== patch.key), merged].sort(byName),
        unreadable: view.unreadable,
      },
    });
  }

  /** Combien de services l'agent a envoyés sans que cette version sache les lire, par serveur (tests, pilotage). */
  setUnreadable(serverId: string, count: number): void {
    const book = this.books.get(serverId);
    if (book?.view.kind !== "supported") return;
    this.unreadable.set(serverId, count);
    this.store(serverId, { ...book.view });
  }

  private store(serverId: string, shown: ServicesViewDto): void {
    const view: ServicesViewDto =
      shown.kind === "supported"
        ? { ...shown, unreadable: this.unreadable.get(serverId) ?? shown.unreadable }
        : shown;
    const seq = (this.books.get(serverId)?.seq ?? 0) + 1;
    // Un état annoncé par le serveur porte ses détails lus : seul `setDetailsKnown(false)` fait le disque.
    this.books.set(serverId, {
      seq,
      view,
      engineReceivedAt: this.received(serverId, view),
      detailsKnown: true,
    });
    this.emit(serverId);
  }

  /** Comme la liaison : une vue du moteur identique à la précédente garde sa date de réception, une autre prend maintenant. */
  private received(serverId: string, view: ServicesViewDto): number | null {
    if (view.kind !== "supported") return null;
    const book = this.books.get(serverId);
    if (
      book &&
      book.view.kind === "supported" &&
      book.engineReceivedAt !== null &&
      JSON.stringify(book.view.engine) === JSON.stringify(view.engine)
    ) {
      return book.engineReceivedAt;
    }
    return this.now();
  }

  private announce(serverId: string): void {
    const book = this.books.get(serverId);
    if (!book) return;
    this.books.set(serverId, { ...book, seq: book.seq + 1 });
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
