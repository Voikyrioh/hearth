import { GIB } from "@/resources";
import type {
  Evidence,
  FicheCard,
  FicheDetail,
  FicheQuestion,
  FicheSetting,
  FicheTheme,
} from "./catalog";
import {
  TERRARIA_DIGEST,
  TERRARIA_SETTINGS,
  TERRARIA_THEMES,
  TERRARIA_VERSION,
} from "./simulated-terraria-settings";

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
      version: TERRARIA_VERSION,
      status: "preview",
      installable: true,
    },
    digest: TERRARIA_DIGEST,
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
        mapping: "nat",
        onlyVariants: [],
      },
    ],
    folders: [
      {
        id: "world",
        role: "world",
        target: "/root/.local/share/Terraria/Worlds",
        backup: true,
        onlyVariants: [],
      },
      { id: "config", role: "config", target: "/config", backup: true, onlyVariants: [] },
    ],
    agreements: [],
    // Les questions de l'installation sont les réglages essentiels que la fiche n'impose pas (BR-FICHE-038).
    questions: structuredClone(TERRARIA_SETTINGS)
      .filter((setting) => setting.level === "essential" && setting.imposedWhy === null)
      .map((setting) => setting.question),
    resources: {
      cpu: { minMillis: null, recommendedMillis: null, evidence: { kind: "unmeasured" } },
      memory: { minBytes: null, recommendedBytes: null, evidence: { kind: "unmeasured" } },
      disk: { installBytes: null, evidence: { kind: "unmeasured" } },
    },
    firstStart: { readyTimeoutSeconds: 1800, worldGeneration: true, downloads: [], stages: [] },
    stop: { graceSeconds: 10, worldSaved: "by_sequence", sequenceTested: true },
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
    themes: structuredClone(TERRARIA_THEMES),
    settings: structuredClone(TERRARIA_SETTINGS),
    // La fiche réelle : `names_setting` et `carried_by_world` de `[worlds]` (HRT-106).
    worldImport: {
      namesSetting: "world_name",
      carriedByWorld: ["world_size", "difficulty", "seed"],
    },
    mods: null,
    settingsUnreadable: 0,
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
        mapping: "nat",
        onlyVariants: [],
      },
      {
        id: "query",
        container: 25566,
        protocol: "udp",
        role: "query",
        publish: "optional",
        defaultHost: 25566,
        mapping: "nat",
        onlyVariants: [],
      },
      // Un port que seule la variante avec mods publie, et que le jeu annonce lui-même : même nombre partout.
      {
        id: "mods_port",
        container: 25575,
        protocol: "tcp",
        role: "voice",
        publish: "always",
        defaultHost: 25575,
        mapping: "same_number",
        onlyVariants: ["modded"],
      },
    ],
    folders: [
      { id: "world", role: "world", target: "/data/world", backup: true, onlyVariants: [] },
      { id: "logs", role: "logs", target: "/data/logs", backup: false, onlyVariants: [] },
      // Un dossier que seule la variante avec mods demande.
      { id: "mods", role: "mods", target: "/data/mods", backup: true, onlyVariants: ["modded"] },
    ],
    agreements: [
      {
        id: "licence",
        title: "Contrat de licence de l'éditeur",
        url: "https://example.org/licence",
        sentence: "L'éditeur exige que vous acceptiez son contrat avant d'installer son serveur.",
      },
    ],
    questions: richQuestions(),
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
    ...richSettings(),
    // La fiche d'essai ne dit pas ce qu'est un monde : la carte « Monde » n'existe pas pour elle (HRT-106).
    worldImport: null,
    unreadable: 0,
  };
}

function setting(
  theme: string,
  level: FicheSetting["level"],
  key: string,
  made: FicheQuestion,
  rest: Partial<Omit<FicheSetting, "question" | "key" | "theme" | "level">> = {},
): FicheSetting {
  return {
    question: made,
    key,
    theme,
    level,
    gameDefault: null,
    imposedWhy: null,
    since: null,
    until: null,
    replacedBy: null,
    list: null,
    ...rest,
  };
}

/**
 * Le catalogue de réglages de la fiche d'essai : tout ce qu'un réglage peut être et que Terraria n'a pas (une liste
 * d'identifiants, un texte opaque, un choix long, un interrupteur, un réglage imposé, un réglage d'une variante, un défaut
 * non vérifié, un réglage ajouté dans une version).
 */
function richSettings(): Pick<FicheDetail, "themes" | "settings" | "settingsUnreadable" | "mods"> {
  const themes: FicheTheme[] = [
    { id: "server", label: "Serveur", order: 1 },
    { id: "gameplay", label: "Partie", order: 2 },
    { id: "mods", label: "Mods", order: 3 },
  ];
  const base = richQuestions();
  const byId = (id: string) => base.find((item) => item.id === id) as FicheQuestion;
  const settings: FicheSetting[] = [
    setting("server", "essential", "server-name", byId("server_name"), { gameDefault: "Serveur" }),
    setting("server", "essential", "slots", byId("slots"), { gameDefault: "10" }),
    setting("server", "essential", "token", byId("server_token")),
    setting(
      "server",
      "common",
      "motd",
      question("motd", "Message d'accueil", "Le message que chaque joueur voit en arrivant.", {
        kind: "text",
        charset: "line",
        minLen: 0,
        maxLen: 60,
      }),
      { gameDefault: "" },
    ),
    setting(
      "server",
      "advanced",
      "region",
      question("region", "Région", "La région annoncée dans la liste publique des serveurs.", {
        kind: "choice",
        proposed: "eu",
        choices: [
          { value: "eu", label: "Europe" },
          { value: "na", label: "Amérique du Nord" },
          { value: "sa", label: "Amérique du Sud" },
          { value: "as", label: "Asie" },
          { value: "oc", label: "Océanie" },
          { value: "af", label: "Afrique" },
        ],
      }),
      { gameDefault: "eu" },
    ),
    setting(
      "server",
      "advanced",
      "online-mode",
      question(
        "online_mode",
        "Vérification des comptes",
        "Le serveur vérifie le compte de chaque joueur.",
        {
          kind: "bool",
          choices: [
            { value: "true", label: "true" },
            { value: "false", label: "false" },
          ],
        },
      ),
      { gameDefault: "true", imposedWhy: "Le serveur vérifie le compte de chaque joueur." },
    ),
    setting("gameplay", "essential", "mode", byId("mode"), { gameDefault: "survival" }),
    setting("gameplay", "essential", "pvp", byId("pvp"), { gameDefault: "false" }),
    setting(
      "gameplay",
      "common",
      "whitelist",
      question("whitelist", "Liste blanche", "Seuls les joueurs de la liste peuvent entrer.", {
        kind: "bool",
        proposed: "false",
        choices: [
          { value: "true", label: "true" },
          { value: "false", label: "false" },
        ],
      }),
      { gameDefault: "false" },
    ),
    setting(
      "gameplay",
      "advanced",
      "damage-scale",
      question("damage_scale", "Force des dégâts", "Multiplie les dégâts, de 0 à 10.", {
        kind: "int",
        proposed: "5",
        min: 0,
        max: 10,
      }),
      { gameDefault: "5" },
    ),
    setting(
      "gameplay",
      "advanced",
      "sandbox",
      question(
        "sandbox_code",
        "Code des options de partie",
        "Réglez vos choix dans le jeu, copiez le code et collez-le ici. Hearth ne peut ni le lire ni le fabriquer.",
        { kind: "opaque", maxLen: 400 },
      ),
      { since: "3.0" },
    ),
    setting(
      "gameplay",
      "advanced",
      "old-difficulty",
      question(
        "old_difficulty",
        "Ancienne difficulté",
        "Remplacée par le code des options de partie.",
        {
          kind: "int",
          min: 0,
          max: 5,
        },
      ),
      { until: "3.0", replacedBy: "sandbox_code" },
    ),
    setting("mods", "essential", "mod-pack", byId("mod_pack")),
    setting(
      "mods",
      "common",
      "mod-ids",
      question("mod_ids", "Numéros des mods", "Les numéros des mods que le jeu charge.", {
        kind: "id_list",
        variants: ["modded"],
      }),
      { list: { itemCharset: "digits", separator: ",", maxItems: 5 } },
    ),
  ];
  // Les voies de mods de la fiche d'essai (HRT-124) : le dépôt d'un pack serveur (avec le moteur à mods) ou Modrinth.
  const mods = {
    ways: ["drop_pack" as const],
    packGenres: [
      { kind: "server_pack" as const, variants: ["modded"] },
      { kind: "modrinth_pack" as const, variants: ["vanilla"] },
    ],
    filesHint: null,
  };
  return { themes, settings, settingsUnreadable: 0, mods };
}

/** Les questions d'installation de la fiche d'essai. */
function richQuestions(): FicheQuestion[] {
  return [
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
  ];
}

/** Les huit fiches en cours de preuve d'un agent de publication, telles que sa liste les montre (HRT-236) : raison écrite pour le joueur, jamais installables. */
export function waitingCards(): FicheCard[] {
  const waiting = (
    id: string,
    name: string,
    summary: string,
    previewReason: string,
  ): FicheCard => ({
    id,
    name,
    summary,
    version: 1,
    status: "preview",
    installable: false,
    previewReason,
  });
  return [
    waiting(
      "zomboid",
      "Project Zomboid",
      "Le serveur officiel de Project Zomboid (Build 42), sans mod.",
      "Le mot de passe d'administration apparaîtrait dans les journaux du jeu. Hearth attend une correction de l'image.",
    ),
    waiting(
      "vrising",
      "V Rising",
      "Le serveur officiel de V Rising, sans mod, par Wine.",
      "Tout fonctionne sauf un point : l'enregistrement du monde avant l'arrêt n'est pas encore vérifié.",
    ),
    waiting(
      "factorio",
      "Factorio",
      "Le serveur officiel de Factorio, avec ou sans l'extension Space Age, sans mod.",
      "Le nom de la partie, son mot de passe et le nombre de joueurs ne se règlent pas encore depuis Hearth.",
    ),
    waiting(
      "corekeeper",
      "Core Keeper",
      "Le serveur officiel de Core Keeper, en connexion directe, sans mod.",
      "Le mot de passe du serveur apparaîtrait dans les journaux du jeu, et le jeu ne s'arrête pas encore proprement.",
    ),
    waiting(
      "7dtd",
      "7 Days to Die",
      "Le serveur officiel de 7 Days to Die, sans mod.",
      "Le jeu ne démarre pas encore avec les protections que Hearth impose à tout serveur.",
    ),
    waiting(
      "palworld",
      "Palworld",
      "Le serveur officiel de Palworld, sans mod.",
      "Le jeu ne s'arrête pas encore proprement : Hearth ne peut pas garantir votre monde à l'arrêt.",
    ),
    waiting(
      "rust",
      "Rust",
      "Le serveur officiel de Rust, sans mod.",
      "Pas encore essayé sur une vraie machine par Hearth, et sa console à distance n'est pas prise en charge : son mot de passe passerait par la ligne de commande.",
    ),
    waiting(
      "satisfactory",
      "Satisfactory",
      "Le serveur officiel de Satisfactory, sans mod.",
      "Le premier joueur qui se connecte pourrait s'approprier le serveur, et le jeu ne s'arrête pas encore proprement.",
    ),
  ];
}

// ── Le catalogue de démonstration (navigateur de développement) ─────────────────────────────────────────────────
// Les douze fiches de l'agent de publication : quatre prêtes (Terraria, Minecraft Java, Valheim, Enshrouded) et huit en
// cours de preuve avec leur raison (`waitingCards`). Les détails des trois nouvelles prêtes sont ABRÉGÉS (quelques
// questions, les ports et ressources de la vraie fiche) : le jugement d'une installation, lui, est celui du simulateur.
// Jamais utilisé par les tests, qui construisent leurs fiches (`terraria()`, `rich()`).

interface GameSpec {
  id: string;
  name: string;
  summary: string;
  version: number;
  digest: string;
  image: { repository: string; tag: string };
  ports: {
    id: string;
    container: number;
    protocol: "tcp" | "udp";
    role: "game" | "query" | "remote_console";
    publish: "always" | "never";
  }[];
  questions: FicheQuestion[];
  memory: { min: number; recommended: number; evidence: Evidence };
  disk: number;
  readyTimeoutSeconds: number;
  stop: FicheDetail["stop"];
  illustration?: boolean;
  steamAppId?: number;
  variants?: FicheDetail["variants"];
}

function publishedGame(spec: GameSpec): FicheDetail {
  const settings = spec.questions.map((made, index) =>
    setting(
      "game",
      made.secret || made.frozenAfterCreation || index < 2 ? "essential" : "common",
      // Ces fiches passent leurs réglages par des variables d'environnement : le nom du réglage est celui de la variable.
      made.id.toUpperCase(),
      made,
    ),
  );
  return {
    card: {
      id: spec.id,
      name: spec.name,
      summary: spec.summary,
      version: spec.version,
      status: "published",
      installable: true,
      memory: { recommendedBytes: spec.memory.recommended, evidence: spec.memory.evidence },
      ...(spec.steamAppId ? { steamAppId: spec.steamAppId } : {}),
      ...(spec.illustration
        ? {
            illustration: {
              digest: `sha256:${spec.digest}`,
              bytes: 120_000,
              width: 920,
              height: 430,
              mediaType: "image/jpeg",
            },
          }
        : {}),
    },
    digest: spec.digest,
    image: { ...spec.image, digest: null },
    ownerUid: 1000,
    ownerGid: 1000,
    variants: spec.variants ?? [],
    ports: spec.ports.map((port) => ({
      id: port.id,
      container: port.container,
      protocol: port.protocol,
      role: port.role,
      publish: port.publish,
      defaultHost: port.publish === "always" ? port.container : null,
      mapping: "nat" as const,
      onlyVariants: [],
    })),
    folders: [{ id: "data", role: "world", target: "/data", backup: true, onlyVariants: [] }],
    agreements: [],
    questions: spec.questions,
    resources: {
      cpu: { minMillis: null, recommendedMillis: null, evidence: { kind: "unmeasured" } },
      memory: {
        minBytes: spec.memory.min,
        recommendedBytes: spec.memory.recommended,
        evidence: spec.memory.evidence,
      },
      disk: { installBytes: spec.disk, evidence: { kind: "measured_idle" } },
    },
    firstStart: {
      readyTimeoutSeconds: spec.readyTimeoutSeconds,
      worldGeneration: true,
      downloads: [],
      stages: [],
    },
    stop: spec.stop,
    imposed: [],
    themes: [{ id: "game", label: "Partie", order: 1 }],
    settings,
    worldImport: null,
    mods: null,
    settingsUnreadable: 0,
    unreadable: 0,
  };
}

/** Minecraft Java, Valheim et Enshrouded : les trois autres fiches prêtes du catalogue de démonstration. */
export function demoPublished(): FicheDetail[] {
  const worldName = (proposed: string) =>
    question("world_name", "Nom du monde", "Le nom du monde que le jeu crée.", {
      kind: "text",
      charset: "file_stem",
      minLen: 1,
      maxLen: 30,
      proposed,
      frozenAfterCreation: true,
    });
  const password = (minLen: number) =>
    question(
      "password",
      "Mot de passe du serveur",
      "Les joueurs le saisissent pour se connecter. Laissé vide, tout le monde peut rejoindre.",
      { kind: "secret", secretKind: "game_password", minLen, maxLen: 30 },
    );
  const serverName = (proposed: string) =>
    question("server_name", "Nom du serveur", "Le nom affiché dans la liste des serveurs.", {
      kind: "text",
      charset: "line",
      minLen: 1,
      maxLen: 40,
      proposed,
    });
  return [
    publishedGame({
      id: "minecraft",
      name: "Minecraft Java",
      summary:
        "Le serveur de Minecraft Java : officiel, Paper, Purpur, Fabric, Quilt, Forge ou NeoForge, avec ou sans pack de mods.",
      version: 4,
      digest: "a1c4e7f0b3d6a9c2e5f8b1d4a7c0e3f6a9c2e5f8b1d4a7c0e3f6a9c2e5f8b1d4",
      image: { repository: "itzg/minecraft-server", tag: "java21" },
      ports: [
        { id: "game", container: 25565, protocol: "tcp", role: "game", publish: "always" },
        {
          id: "rcon",
          container: 25575,
          protocol: "tcp",
          role: "remote_console",
          publish: "never",
        },
      ],
      questions: [
        question(
          "motd",
          "Message d'accueil",
          "Le message que les joueurs voient dans la liste des serveurs.",
          {
            kind: "text",
            charset: "line",
            minLen: 0,
            maxLen: 60,
            proposed: "Un serveur Minecraft géré par Hearth",
          },
        ),
        worldName("world"),
        question("difficulty", "Difficulté", "La difficulté du monde.", {
          kind: "choice",
          proposed: "normal",
          choices: [
            { value: "peaceful", label: "Pacifique" },
            { value: "easy", label: "Facile" },
            { value: "normal", label: "Normal" },
            { value: "hard", label: "Difficile" },
          ],
        }),
        question(
          "max_players",
          "Nombre de joueurs",
          "Combien de joueurs peuvent être connectés en même temps.",
          { kind: "int", min: 1, max: 200, proposed: "20" },
        ),
      ],
      memory: { min: 2 * GIB, recommended: 4 * GIB, evidence: { kind: "measured" } },
      disk: 1_497_623_985,
      readyTimeoutSeconds: 3600,
      stop: { graceSeconds: 60, worldSaved: "by_sequence", sequenceTested: true },
      illustration: true,
      // Le moteur du serveur se choisit à l'installation, comme dans la vraie fiche (une variante par moteur).
      variants: [
        {
          id: "vanilla",
          label: "Officiel",
          help: "Le serveur de Mojang, sans mod.",
          default: true,
        },
        {
          id: "paper",
          label: "Paper",
          help: "Un serveur rapide, avec des extensions.",
          default: false,
        },
        {
          id: "purpur",
          label: "Purpur",
          help: "Paper, avec encore plus de réglages.",
          default: false,
        },
        {
          id: "fabric",
          label: "Fabric",
          help: "Un serveur léger pour les mods Fabric.",
          default: false,
        },
        { id: "quilt", label: "Quilt", help: "Un serveur pour les mods Quilt.", default: false },
        { id: "forge", label: "Forge", help: "Un serveur pour les mods Forge.", default: false },
        {
          id: "neoforge",
          label: "NeoForge",
          help: "Un serveur pour les mods NeoForge.",
          default: false,
        },
      ],
    }),
    publishedGame({
      id: "valheim",
      name: "Valheim",
      summary: "Le serveur officiel de Valheim, sans mod.",
      version: 1,
      digest: "b2d5f8a1c4e7b0d3f6a9c2e5b8d1f4a7c0e3b6d9f2a5c8e1b4d7f0a3c6e9b2d5",
      image: { repository: "ghcr.io/lloesche/valheim-server", tag: "latest" },
      ports: [
        { id: "game", container: 2456, protocol: "udp", role: "game", publish: "always" },
        { id: "query", container: 2457, protocol: "udp", role: "query", publish: "always" },
      ],
      questions: [serverName("Mon serveur Valheim"), password(5), worldName("Dedicated")],
      memory: { min: 4 * GIB, recommended: 6 * GIB, evidence: { kind: "measured" } },
      disk: 4_743_565_597,
      readyTimeoutSeconds: 1800,
      stop: { graceSeconds: 30, worldSaved: "by_image_on_signal", sequenceTested: true },
      steamAppId: 892970,
    }),
    publishedGame({
      id: "enshrouded",
      name: "Enshrouded",
      summary: "Le serveur officiel d'Enshrouded, sans mod, par Wine.",
      version: 1,
      digest: "c3e6a9b2d5f8c1e4a7b0d3f6c9e2a5b8d1f4c7a0e3b6d9f2c5a8e1b4d7f0c3a6",
      image: { repository: "mornedhels/enshrouded-server", tag: "latest" },
      ports: [{ id: "game", container: 15637, protocol: "udp", role: "game", publish: "always" }],
      questions: [
        serverName("Mon serveur Enshrouded"),
        password(0),
        question(
          "slots",
          "Nombre de joueurs",
          "Combien de joueurs peuvent être connectés en même temps.",
          { kind: "int", min: 1, max: 16, proposed: "16" },
        ),
      ],
      memory: {
        min: 16 * GIB,
        recommended: 16 * GIB,
        evidence: { kind: "publisher", url: "https://github.com/mornedhels/enshrouded-server" },
      },
      disk: 8_946_101_770,
      readyTimeoutSeconds: 3600,
      stop: { graceSeconds: 30, worldSaved: "by_image_on_signal", sequenceTested: true },
      steamAppId: 1203620,
    }),
  ];
}

/** Terraria tel que sa fiche livrée le dit (version 5) : mémoire mesurée sur la limite essayée, disque mesuré, image Steam. */
export function demoTerraria(detail: FicheDetail): void {
  detail.card.memory = { recommendedBytes: 2 * GIB, evidence: { kind: "measured" } };
  detail.card.status = "published";
  detail.card.steamAppId = 105600;
  detail.resources.memory = {
    minBytes: 2 * GIB,
    recommendedBytes: 2 * GIB,
    evidence: { kind: "measured" },
  };
  detail.resources.disk = { installBytes: 53_014_364, evidence: { kind: "measured_idle" } };
}
