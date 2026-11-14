import type { GameReadDto, GameSecretDto } from "@/bindings";
import type { FicheStop } from "./catalog";

/**
 * Le quotidien d'un serveur de jeu (HRT-104, `BR-JEU-017` à `BR-JEU-021`, `BR-JEU-025` à `BR-JEU-027`) : types de
 * l'interface, côté pont. Deux LECTURES :
 *
 * - `readGameView` : la fiche dont le service est né, la façon dont son monde est enregistré à l'arrêt, les ports
 *   publiés, et SI le serveur a un mot de passe (jamais sa valeur). Ouverte à tout compte ; `absent` : ce service n'est
 *   pas un serveur de jeu créé depuis une fiche (adopté, ordinaire, agent plus ancien), jamais une panne ;
 * - `revealGamePassword` : la valeur du mot de passe, réservée au droit « gérer les services », consignée au journal par
 *   l'agent. Elle ne traverse que cet appel : ni gardée, ni tracée, effacée au remasquage.
 */

/** Un port publié par le serveur de jeu : le numéro à ouvrir sur la box, pas celui du service. */
export interface GamePort {
  hostPort: number;
  /** `tcp`, `udp` ou `sctp` ; une valeur plus récente que ce client est écartée à l'affichage. */
  protocol: string;
  /** `game`, `query`, `voice`, `web_admin`, `remote_console`, `other` ou vide (la fiche ne reconnaît plus ce port). */
  role: string;
  /** Le numéro diffère de celui que propose la fiche (`BR-JEU-019`). */
  changed: boolean;
}

export interface GameView {
  /** L'identifiant de la fiche. */
  fiche: string;
  /** Le nom du jeu (« Terraria »). */
  name: string;
  ficheVersion: number;
  stop: FicheStop;
  ports: GamePort[];
  hasPassword: boolean;
}

export type GameRead = { kind: "found"; view: GameView } | { kind: "absent" };

/** Le mot de passe du serveur ; `null` : le serveur n'en a pas (`BR-JEU-027`). */
export interface GameSecret {
  password: string | null;
}

export function toGameRead(dto: GameReadDto): GameRead {
  if (dto.kind === "absent") return { kind: "absent" };
  const view = dto.view;
  return {
    kind: "found",
    view: {
      fiche: view.fiche,
      name: view.name,
      ficheVersion: view.ficheVersion,
      stop: { ...view.stop },
      ports: view.ports.map((port) => ({ ...port })),
      hasPassword: view.hasPassword,
    },
  };
}

export function toGameSecret(dto: GameSecretDto): GameSecret {
  return { password: dto.password ?? null };
}
