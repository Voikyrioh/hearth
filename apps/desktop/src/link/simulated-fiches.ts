import { GIB } from "@/resources";
import type { FicheDetail, FicheQuestion } from "./catalog";

/**
 * Les fiches du pont SIMULÉ (navigateur de développement, Vitest, Playwright). Jamais livré (derrière
 * `import.meta.env.DEV`).
 *
 * - `terraria()` a la forme RÉELLE de `GET /catalog/terraria` d'un agent d'essai (fiche version 2 de
 *   `crates/hearth-agent/catalog/terraria/`) : six réglages essentiels, toutes les ressources « non mesurées », aucune
 *   variante, aucun contrat, un port ;
 * - `rich()` est une fiche d'essai qui porte tout ce qu'une fiche peut offrir et que Terraria n'a pas : des variantes,
 *   un contrat, un port facultatif, des ressources mesurées, au repos et annoncées par l'éditeur, un jeton de serveur.
 *   Elle sert à prouver que l'écran n'a aucune règle propre à un jeu.
 */

const DIGEST_TERRARIA = "7c1f0d5a9e3b24c6a8d0f1e2b3a495867c1f0d5a9e3b24c6a8d0f1e2b3a49586";
const DIGEST_RICH = "e4b2a1c3d5f60718293a4b5c6d7e8f90e4b2a1c3d5f60718293a4b5c6d7e8f90";

function question(
  id: string,
  label: string,
  help: string,
  rest: Partial<FicheQuestion> & Pick<FicheQuestion, "kind">,
): FicheQuestion {
  return {
    id,
    label,
    help,
    secret: rest.kind === "secret",
    proposed: null,
    choices: [],
    min: null,
    max: null,
    minLen: null,
    maxLen: null,
    charset: null,
    frozenAfterCreation: false,
    variants: [],
    secretKind: null,
    obtainUrl: null,
    ...rest,
  };
}

export function terraria(): FicheDetail {
  return {
    card: {
      id: "terraria",
      name: "Terraria",
      summary: "Le serveur officiel de Terraria, sans mod.",
      version: 2,
      status: "preview",
      installable: true,
    },
    digest: DIGEST_TERRARIA,
    image: { repository: "ryshe/terraria", tag: "vanilla-1.4.5.8", digest: null },
    ownerUid: 0,
    ownerGid: 0,
    variants: [],
    ports: [
      {
        id: "game",
        container: 7777,
        protocol: "tcp",
        role: "game",
        publish: "always",
        defaultHost: 7777,
      },
    ],
    folders: [
      {
        id: "world",
        role: "world",
        target: "/root/.local/share/Terraria/Worlds",
        backup: true,
      },
      { id: "config", role: "config", target: "/config", backup: true },
    ],
    agreements: [],
    questions: [
      question(
        "world_name",
        "Nom du monde",
        "Le nom du monde que le jeu crée au premier lancement. Il nomme aussi le fichier du monde : lettres, chiffres, tiret et souligné seulement. Il ne peut plus changer une fois le monde créé.",
        {
          kind: "text",
          proposed: "Monde",
          charset: "file_stem",
          minLen: 0,
          maxLen: 100,
          frozenAfterCreation: true,
        },
      ),
      question(
        "password",
        "Mot de passe du serveur",
        "Ce que vos joueurs tapent pour entrer sur le serveur. Laissez vide pour que tout le monde puisse entrer. Hearth ne l'affiche jamais.",
        { kind: "secret", secretKind: "game_password", minLen: 0, maxLen: 100 },
      ),
      question(
        "motd",
        "Message d'accueil",
        "Le message que chaque joueur voit en arrivant sur le serveur. Laissez vide pour n'afficher aucun message.",
        { kind: "text", charset: "line", minLen: 0, maxLen: 100 },
      ),
      question(
        "max_players",
        "Nombre de joueurs",
        "Combien de joueurs peuvent être connectés en même temps, de 1 à 255. Plus il y en a, plus le serveur travaille : surveillez l'onglet Ressources.",
        { kind: "int", proposed: "8", min: 1, max: 255 },
      ),
      question(
        "world_size",
        "Taille du monde",
        "La taille du monde que le jeu crée s'il n'existe pas encore : petit, moyen ou grand. Elle est gravée dans le monde et ne change plus ensuite.",
        {
          kind: "choice",
          proposed: "3",
          choices: [
            { value: "1", label: "Petit" },
            { value: "2", label: "Moyen" },
            { value: "3", label: "Grand" },
          ],
          frozenAfterCreation: true,
        },
      ),
      question(
        "difficulty",
        "Difficulté",
        "La difficulté du monde créé : classique, expert, maître ou voyage. Elle est gravée dans le monde et ne change plus ensuite.",
        {
          kind: "choice",
          proposed: "2",
          choices: [
            { value: "0", label: "Classique" },
            { value: "1", label: "Expert" },
            { value: "2", label: "Maître" },
            { value: "3", label: "Voyage" },
          ],
          frozenAfterCreation: true,
        },
      ),
    ],
    resources: {
      cpu: { minMillis: null, recommendedMillis: null, evidence: { kind: "unmeasured" } },
      memory: { minBytes: null, recommendedBytes: null, evidence: { kind: "unmeasured" } },
      disk: { installBytes: null, evidence: { kind: "unmeasured" } },
    },
    firstStart: { readyTimeoutSeconds: 1800, worldGeneration: true, downloads: [], stages: [] },
    stop: { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: false },
    imposed: [
      {
        label: "Nom du fichier du monde",
        value: null,
        why: "Hearth le déduit du nom du monde pour que le jeu retrouve son monde à chaque démarrage.",
      },
      {
        label: "Port du serveur",
        value: "7777",
        why: "Le port du jeu dans le service : celui que vous choisissez est relié à celui-ci.",
      },
      {
        label: "Ouverture automatique du port (UPnP)",
        value: "désactivée",
        why: "Un service ne modifie pas la box de votre réseau.",
      },
    ],
    unreadable: 0,
  };
}

export function rich(): FicheDetail {
  return {
    card: {
      id: "essai",
      name: "Jeu d'essai",
      summary: "Une fiche d'essai qui offre tout ce qu'une fiche peut offrir.",
      version: 3,
      status: "preview",
      installable: true,
    },
    digest: DIGEST_RICH,
    image: {
      repository: "example/essai",
      tag: "2.4.1",
      digest: "sha256:0a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9",
    },
    ownerUid: 1000,
    ownerGid: 1000,
    variants: [
      {
        id: "vanilla",
        label: "Sans mod",
        help: "Le serveur du jeu, tel que l'éditeur le publie.",
        default: true,
      },
      {
        id: "modded",
        label: "Avec mods",
        help: "Le serveur avec un chargeur de mods.",
        default: false,
      },
    ],
    ports: [
      {
        id: "game",
        container: 25565,
        protocol: "tcp",
        role: "game",
        publish: "always",
        defaultHost: 25565,
      },
      {
        id: "query",
        container: 25566,
        protocol: "udp",
        role: "query",
        publish: "optional",
        defaultHost: 25566,
      },
    ],
    folders: [
      { id: "world", role: "world", target: "/data/world", backup: true },
      { id: "logs", role: "logs", target: "/data/logs", backup: false },
    ],
    agreements: [
      {
        id: "licence",
        title: "Contrat de licence de l'éditeur",
        url: "https://example.org/licence",
        sentence: "L'éditeur exige que vous acceptiez son contrat avant d'installer son serveur.",
      },
    ],
    questions: [
      question("server_name", "Nom du serveur", "Le nom que voient les joueurs.", {
        kind: "text",
        proposed: "Mon serveur",
        charset: "line",
        minLen: 1,
        maxLen: 40,
      }),
      question(
        "slots",
        "Nombre de places",
        "Combien de joueurs peuvent se connecter en même temps.",
        { kind: "int", proposed: "10", min: 2, max: 64 },
      ),
      question("mode", "Mode de jeu", "Le mode de la partie.", {
        kind: "choice",
        proposed: "survival",
        choices: [
          { value: "survival", label: "Survie" },
          { value: "creative", label: "Créatif" },
        ],
        frozenAfterCreation: true,
      }),
      question("pvp", "Combat entre joueurs", "Autorise les joueurs à se battre entre eux.", {
        kind: "bool",
        proposed: "false",
        // Les deux valeurs que le jeu comprend : activé, puis désactivé.
        choices: [
          { value: "true", label: "true" },
          { value: "false", label: "false" },
        ],
      }),
      question(
        "server_token",
        "Jeton de serveur",
        "Le jeton propre à ce serveur, créé sur le site de la plateforme.",
        {
          kind: "secret",
          secretKind: "server_token",
          minLen: 1,
          maxLen: 64,
          obtainUrl: "https://example.org/jetons",
        },
      ),
      question("mod_pack", "Pack de mods", "Le pack chargé au démarrage.", {
        kind: "text",
        charset: "arg_safe",
        proposed: "base",
        minLen: 1,
        maxLen: 30,
        variants: ["modded"],
      }),
    ],
    resources: {
      cpu: {
        minMillis: 1000,
        recommendedMillis: 2000,
        evidence: { kind: "measured" },
      },
      memory: {
        minBytes: 2 * GIB,
        recommendedBytes: 4 * GIB,
        evidence: { kind: "measured_idle" },
      },
      disk: {
        installBytes: 8 * GIB,
        evidence: { kind: "publisher", url: "https://example.org/configuration" },
      },
    },
    firstStart: {
      readyTimeoutSeconds: 900,
      worldGeneration: false,
      downloads: [
        {
          what: "fichiers de jeu",
          source: "les serveurs de l'éditeur",
          bytes: 6 * GIB,
          evidence: { kind: "publisher", url: "https://example.org/taille" },
        },
      ],
      stages: ["Téléchargement des fichiers de jeu", "Chargement du monde"],
    },
    stop: { graceSeconds: 30, worldSaved: "by_image_on_signal", sequenceTested: true },
    imposed: [
      {
        label: "Mode hors ligne",
        value: "désactivé",
        why: "Le serveur vérifie le compte de chaque joueur.",
      },
    ],
    unreadable: 0,
  };
}
