import type { FicheDetail } from "./catalog";
import type { GamePort, GameRead, GameSecret, GameView } from "./game";

/**
 * Le quotidien des serveurs de jeu du pont SIMULÉ (HRT-104) : ce que l'agent garde d'un service né d'une fiche, et
 * que l'appelant PILOTE (navigateur de développement, Vitest, Playwright). Un service installé depuis une fiche s'y
 * inscrit tout seul (`registerInstalled`) ; un service semé par un test s'y inscrit par `register`. Un service qui n'y
 * figure pas (adopté, ordinaire) se lit `absent` (BR-JEU-001). Le mot de passe est tenu à part de la vue : elle ne
 * dit que « oui » ou « non », comme l'agent. Jamais livré (derrière `import.meta.env.DEV`).
 */

interface Held {
  view: GameView;
  password: string | null;
}

export class SimulatedGames {
  private readonly held = new Map<string, Held>();
  /** Les lectures du mot de passe reçues, dans l'ordre (de quoi vérifier qu'aucune n'est partie sans geste). */
  readonly reveals: { serverId: string; key: string }[] = [];
  /** Les lectures de la vue reçues. */
  readonly reads: { serverId: string; key: string }[] = [];
  /** `true` : la prochaine lecture de la vue échoue (agent qui ne répond pas). */
  failRead = false;
  /** `true` : la prochaine révélation échoue. */
  failReveal = false;

  private id(serverId: string, key: string): string {
    return `${serverId}/${key}`;
  }

  /** Inscrit un service de jeu : `view` est ce que l'agent en dit, `password` la valeur qu'il garde (`null` : aucune). */
  register(
    serverId: string,
    key: string,
    view: Partial<GameView> & { name: string },
    password: string | null = null,
  ): void {
    this.held.set(this.id(serverId, key), {
      view: {
        fiche: "terraria",
        ficheVersion: 1,
        stop: { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: false },
        ports: [{ hostPort: 7777, protocol: "tcp", role: "game", changed: false }],
        hasPassword: password !== null && password !== "",
        ...view,
      },
      password: password === "" ? null : password,
    });
  }

  /** Un service installé depuis une fiche : ses ports sont ceux que la fiche publie, le mot de passe est la réponse donnée. */
  registerInstalled(
    serverId: string,
    key: string,
    detail: FicheDetail,
    answers: readonly { id: string; value: string }[],
  ): void {
    const ports: GamePort[] = detail.ports
      .filter((port) => port.publish === "always")
      .map((port) => ({
        hostPort: port.defaultHost ?? port.container,
        protocol: port.protocol,
        role: port.role,
        changed: false,
      }));
    const password = answers.find((answer) => answer.id === "password")?.value ?? null;
    this.register(
      serverId,
      key,
      {
        fiche: detail.card.id,
        name: detail.card.name,
        ficheVersion: detail.card.version,
        stop: { ...detail.stop } as GameView["stop"],
        ports,
      },
      password,
    );
  }

  has(serverId: string, key: string): boolean {
    return this.held.has(this.id(serverId, key));
  }

  /** La façon d'arrêter de ce service, telle que la fiche la déclare, ou `null` si ce n'est pas un service de jeu. */
  stopOf(serverId: string, key: string): GameView["stop"] | null {
    return this.held.get(this.id(serverId, key))?.view.stop ?? null;
  }

  /** Change ce que l'agent dit d'un service (un port qui change, l'essai d'une suite d'arrêt qui passe). */
  patch(serverId: string, key: string, patch: Partial<GameView>): void {
    const entry = this.held.get(this.id(serverId, key));
    if (entry) entry.view = { ...entry.view, ...patch };
  }

  read(serverId: string, key: string): GameRead {
    this.reads.push({ serverId, key });
    const entry = this.held.get(this.id(serverId, key));
    if (!entry) return { kind: "absent" };
    return { kind: "found", view: structuredClone(entry.view) };
  }

  reveal(serverId: string, key: string): GameSecret {
    this.reveals.push({ serverId, key });
    return { password: this.held.get(this.id(serverId, key))?.password ?? null };
  }
}
