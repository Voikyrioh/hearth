import type {
  WorldInspection,
  WorldInspectRead,
  WorldRef,
  WorldRefusal,
  WorldRoot,
  WorldsRead,
  WorldsView,
} from "./worlds";

/**
 * L'agent simulé de la reprise d'un monde (HRT-106, `BR-MONDE-002` à `016`, `039`) : il juge comme le vrai (dossier
 * ouvert, chemin relatif, monde reconnu, nom accepté), liste ce que le test lui a posé et mesure ce que le test lui a
 * dit. La copie, elle, est une étape du déploiement, pilotée par l'appelant (`SimulatedDeploys.worldProgress`).
 */
const GIB = 1024 * 1024 * 1024;
const MAX_DEPTH = 3;
const MAX_ENTRIES = 5000;

/** Un chemin relatif : ni absolu, ni vide, ni `..` (BR-MONDE-002). */
function isRelative(path: string): boolean {
  return (
    path !== "" &&
    !path.startsWith("/") &&
    !/^[a-zA-Z]:/.test(path) &&
    !path.split(/[\\/]/).some((part) => part === ".." || part === "")
  );
}

export class SimulatedWorlds {
  private readonly views = new Map<string, WorldsView>();
  private readonly inspections = new Map<string, Partial<WorldInspection>>();
  /** Les lectures de liste, dans l'ordre. */
  readonly listed: { serverId: string; ficheId: string }[] = [];
  /** Les inspections reçues, dans l'ordre : de quoi vérifier que la liste est lue une fois et que rien ne part sans choix. */
  readonly inspected: { serverId: string; ficheId: string; world: WorldRef }[] = [];
  /** `true` : la lecture de la liste échoue (lien sans réponse). */
  failList = false;
  /** `true` : l'inspection échoue. */
  failInspect = false;
  /** `true` : l'agent n'a pas la capacité `world-import` (un agent d'avant HRT-105). */
  unavailable = false;

  /** `knows` : la fiche déclare ce qu'est un monde (`BR-MONDE-039`). */
  constructor(private readonly knows: (ficheId: string) => boolean) {}

  private slot(serverId: string, ficheId: string): string {
    return `${serverId}/${ficheId}`;
  }

  /** Pose les dossiers ouverts aux services et les mondes que la recherche y trouve. */
  seed(serverId: string, ficheId: string, roots: WorldRoot[]): void {
    this.views.set(this.slot(serverId, ficheId), {
      roots: structuredClone(roots),
      maxDepth: MAX_DEPTH,
      maxEntries: MAX_ENTRIES,
    });
  }

  /** Ce que la mesure rendra pour ce monde (le reste prend des valeurs plausibles). */
  setInspection(
    serverId: string,
    folder: string,
    path: string,
    inspection: Partial<WorldInspection>,
  ): void {
    this.inspections.set(`${serverId}|${folder}|${path}`, inspection);
  }

  /** `GET /catalog/{id}/worlds`. */
  read(serverId: string, ficheId: string): WorldsRead {
    this.listed.push({ serverId, ficheId });
    if (this.failList) throw new Error("liste des mondes illisible");
    if (!this.knows(ficheId)) return { kind: "not_supported" };
    const view = this.views.get(this.slot(serverId, ficheId));
    return {
      kind: "ready",
      view: view
        ? structuredClone(view)
        : { roots: [], maxDepth: MAX_DEPTH, maxEntries: MAX_ENTRIES },
    };
  }

  /** Pourquoi l'agent refuse ce monde, ou `null` : il existe, dans un dossier ouvert, avec un nom accepté. */
  judge(serverId: string, ficheId: string, world: WorldRef): WorldRefusal | null {
    const view = this.views.get(this.slot(serverId, ficheId));
    const root = view?.roots.find((item) => item.folder === world.folder);
    if (!root) return "unknown_folder";
    if (!isRelative(world.path)) return "bad_path";
    const entry = root.worlds.find((item) => item.path === world.path);
    if (!entry) return "not_a_world";
    if (!root.readable) return "unreadable";
    if (!entry.usable) return "name_unusable";
    return null;
  }

  /** `POST /catalog/{id}/worlds/inspect`. */
  inspect(serverId: string, ficheId: string, world: WorldRef, freeBytes: number): WorldInspectRead {
    this.inspected.push({ serverId, ficheId, world: structuredClone(world) });
    if (this.failInspect) throw new Error("inspection illisible");
    return this.measure(serverId, ficheId, world, freeBytes);
  }

  private measure(
    serverId: string,
    ficheId: string,
    world: WorldRef,
    freeBytes: number,
  ): WorldInspectRead {
    const refused = this.judge(serverId, ficheId, world);
    if (refused) return { kind: "refused", reason: refused };
    const entry = this.views
      .get(this.slot(serverId, ficheId))
      ?.roots.find((root) => root.folder === world.folder)
      ?.worlds.find((item) => item.path === world.path);
    const posed = this.inspections.get(`${serverId}|${world.folder}|${world.path}`) ?? {};
    return {
      kind: "ready",
      inspection: {
        files: entry?.files ?? 1,
        bytes: entry?.bytes ?? 0.3 * GIB,
        notCopied: 0,
        notCopiedNames: [],
        skippedLinks: 0,
        skippedLinkNames: [],
        partial: false,
        freeBytes,
        ...structuredClone(posed),
      },
    };
  }

  /** Ce que la mesure dit de ce monde, pour juger l'espace d'une installation (n'est pas une inspection du client). */
  measured(
    serverId: string,
    ficheId: string,
    world: WorldRef,
    freeBytes: number,
  ): WorldInspection | null {
    const read = this.measure(serverId, ficheId, world, freeBytes);
    return read.kind === "ready" ? read.inspection : null;
  }
}
