import type { FicheDetail } from "./catalog";
import type { GamePort, GameRead, GameSecret, GameView, PublicAddressOutcome } from "./game";

/**
 * Le quotidien des serveurs de jeu du pont SIMULÉ (HRT-104) : ce que l'agent garde d'un service né d'une fiche, et
 * que l'appelant PILOTE (navigateur de développement, Vitest, Playwright). Un service installé depuis une fiche s'y
 * inscrit tout seul (`registerInstalled`) ; un service semé par un test s'y inscrit par `register`. Un service qui n'y
 * figure pas (adopté, ordinaire) se lit `absent` (BR-JEU-001). Le mot de passe est tenu à part de la vue : elle ne
 * dit que « oui » ou « non », comme l'agent. Jamais livré (derrière `import.meta.env.DEV`).
 */

/** Un refus d'agent que le test peut provoquer (le reste vient du jugement de l'agent simulé). */
export type PublicAddressRefusalKind =
  | "conflict"
  | "service_busy"
  | "gone"
  | "engine_unavailable"
  | "other";

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
  /** `true` : TOUTES les lectures de la vue échouent tant qu'il n'est pas remis à `false` (agent qui ne répond pas). */
  failRead = false;
  /** `true` : TOUTES les révélations échouent tant qu'il n'est pas remis à `false`. */
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
        publicAddress: null,
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
        stop: {
          ...(detail.stop ?? { graceSeconds: 10, worldSaved: "unknown", sequenceTested: false }),
        },
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

  /** Les changements d'adresse reçus (sans le mot de passe), dans l'ordre. */
  readonly addressChanges: { serverId: string; key: string; address: string }[] = [];
  /** Le refus que le prochain changement d'adresse rendra (puis oublié), pour un test : l'agent juge d'abord, ensuite ceci. */
  nextAddressRefusal: PublicAddressRefusalKind | null = null;

  /**
   * `PUT /services/{key}/spec` quand il ne change que l'adresse des joueurs : l'agent juge comme le vrai (service connu,
   * forme de l'adresse : 253 caractères, ni espace ni caractère de contrôle), puis garde l'adresse sans rien recréer.
   * `""` l'efface (BR-JEU-022).
   */
  setPublicAddress(
    serverId: string,
    key: string,
    address: string,
    _by: string,
  ): PublicAddressOutcome {
    this.addressChanges.push({ serverId, key, address });
    const entry = this.held.get(this.id(serverId, key));
    if (!entry) return { kind: "refused", refusal: { kind: "gone" } };
    if (this.nextAddressRefusal !== null) {
      const kind = this.nextAddressRefusal;
      this.nextAddressRefusal = null;
      return {
        kind: "refused",
        refusal: kind === "service_busy" ? { kind, by: "claire" } : ({ kind } as never),
      };
    }
    const hasControl = [...address].some(
      (char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127,
    );
    if ([...address].length > 253 || hasControl || /\s/u.test(address)) {
      return { kind: "refused", refusal: { kind: "invalid" } };
    }
    entry.view = { ...entry.view, publicAddress: address === "" ? null : address };
    return { kind: "saved" };
  }

  reveal(serverId: string, key: string): GameSecret {
    this.reveals.push({ serverId, key });
    return { password: this.held.get(this.id(serverId, key))?.password ?? null };
  }
}
