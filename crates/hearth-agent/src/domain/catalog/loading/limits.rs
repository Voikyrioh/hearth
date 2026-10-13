//! Bornes d'une fiche (invariant I9) : une fiche est du contenu venu d'ailleurs, elle ne doit épuiser ni
//! l'agent ni l'écran. Toutes sont vérifiées au chargement, avant toute allocation proportionnelle.

/// Taille cumulée de `fiche.toml` et `settings.toml`, vérifiée AVANT l'analyse.
pub const MAX_FILES_BYTES: usize = 256 * 1024;
pub const MAX_SETTINGS: usize = 400;
pub const MAX_RETIRED: usize = 200;
pub const MAX_PORTS: usize = 16;
pub const MAX_FOLDERS: usize = 8;
pub const MAX_CHANNELS: usize = 6;
pub const MAX_STOP_STEPS: usize = 6;
/// Durée cumulée au pire d'une séquence (somme des attentes et des délais de journaux), en secondes.
pub const MAX_SEQUENCE_SECONDS: u32 = 20 * 60;
pub const MAX_WAIT_LOG_SECONDS: u32 = 300;
pub const MAX_WAIT_SECONDS: u32 = 120;
pub const MIN_GRACE_SECONDS: u32 = 10;
pub const MAX_GRACE_SECONDS: u32 = 600;
pub const MIN_READY_TIMEOUT_SECONDS: u32 = 60;
pub const MAX_READY_TIMEOUT_SECONDS: u32 = 6 * 60 * 60;

pub const MAX_THEMES: usize = 16;
pub const MAX_CHOICE_SETS: usize = 32;
pub const MAX_CHOICES: usize = 64;
pub const MAX_PRESETS: usize = 16;
pub const MAX_VARIANTS: usize = 8;
pub const MAX_ENV: usize = 32;
pub const MAX_SECRETS: usize = 8;
pub const MAX_AGREEMENTS: usize = 4;
pub const MAX_NOT_EXPOSED: usize = 100;
pub const MAX_CHECKS: usize = 64;
pub const MAX_MIGRATIONS: usize = 32;
pub const MAX_MIGRATION_OPS: usize = 64;
pub const MAX_DOWNLOADS: usize = 4;
pub const MAX_STAGES: usize = 16;
pub const MAX_COMPANIONS: usize = 8;
pub const MAX_ESSENTIAL: usize = 8;

pub const MAX_NAME_CHARS: usize = 60;
pub const MAX_SUMMARY_CHARS: usize = 160;
pub const MAX_LABEL_CHARS: usize = 80;
pub const MAX_HELP_CHARS: usize = 300;
pub const MAX_WHY_CHARS: usize = 300;
pub const MAX_ID_CHARS: usize = 48;
pub const MAX_FICHE_ID_CHARS: usize = 32;
pub const MIN_FICHE_ID_CHARS: usize = 2;
pub const MAX_PATH_CHARS: usize = 128;
pub const MAX_PATH_COMPONENT_CHARS: usize = 64;
pub const MAX_TARGET_CHARS: usize = 200;
pub const MAX_LITERAL_CHARS: usize = 200;
pub const MAX_URL_CHARS: usize = 300;
pub const MAX_TEXT_VALUE_CHARS: usize = 4096;
pub const MAX_ID_LIST_ITEMS: usize = 200;
pub const MAX_ENV_VALUE_CHARS: usize = 256;

/// Un ordre envoyé au jeu par l'entrée standard : 64 caractères ASCII imprimables au plus.
pub const MAX_ORDER_CHARS: usize = 64;
/// Une valeur d'argument de commande : 64 caractères au plus (alphabet fermé).
pub const MAX_ARG_VALUE_CHARS: usize = 64;
/// La version du jeu lue dans les journaux : 32 caractères au plus.
pub const MAX_GAME_VERSION_CHARS: usize = 32;
pub const MIN_GENERATED_SECRET_LENGTH: u32 = 24;
pub const MAX_GENERATED_SECRET_LENGTH: u32 = 64;
/// Premier port que Hearth accepte de publier ou de faire écouter à un jeu.
pub const MIN_PORT: u16 = 1024;
