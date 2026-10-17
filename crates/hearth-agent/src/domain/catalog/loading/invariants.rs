//! Les neuf invariants d'une fiche (conception technique, section 6), séparés du chargeur pour que chaque
//! motif de refus ait sa fiche piégée et son test.
//!
//! | # | Invariant | Où |
//! |---|---|---|
//! | I1 | une seule voie, plan fermé (aucun champ de privilège, image épinglée, port ≥ 1024) | `check_fiche_file` (+ format fermé) |
//! | I2 | tous les montages sont des dossiers gérés du service | `check_fiche_file` |
//! | I3 | aucun texte n'atteint un interpréteur (arguments, ordres, valeurs dérivées) | `check_fiche_file`, `check_cross` |
//! | I4 | aucune exécution : aucun genre d'étape qui lance une commande | `check_steps` |
//! | I5 | aucune adresse contactée : `https` seulement, affichée | `check_url` |
//! | I6 | ni expression régulière, ni gabarit : signes littéraux | `check_literal` |
//! | I7 | une console distante n'est jamais publiée, son mot de passe est généré | `check_fiche_file`, `check_cross` |
//! | I8 | un contrat ne s'accepte que par un geste | `check_fiche_file`, `check_cross` |
//! | I9 | bornes | `check_bounds` |

use std::collections::BTreeSet;

use super::error::{FicheRefusee, Fichier, Motif};
use super::limits::*;
use super::loader::{check_text, check_variant_refs};
use super::memory::{TierId, recommended_gap};
use super::model::{Fiche, Setting, ValueSpec};
use super::schema::fiche::{
    Channel, ChannelKind, Derive, Evidence, FicheFile, FolderRole, MemoryTier, PlainEvidence,
    PortRole, Proof, Publish, Status, Step, StepKind, WorldSaved,
};
use super::schema::settings::{ArgForm, SettingsFile};
use super::value::{
    ChannelPath, Charset, PathRefused, UrlCheck, arg_flag_ok, container_target_ok,
    first_refused_char, https_url_ok, ident_ok, is_risky_char, literal_ok, order_refusal,
};

fn refuse(fichier: Fichier, chemin: impl Into<String>, motif: Motif) -> FicheRefusee {
    FicheRefusee::new(fichier, chemin, motif)
}

// ---- I9 : bornes --------------------------------------------------------------------------------------

fn bound(quoi: &'static str, limite: usize, trouve: usize) -> Result<(), FicheRefusee> {
    if trouve > limite {
        Err(refuse(
            Fichier::Fiche,
            "",
            Motif::BorneDepassee {
                quoi,
                limite,
                trouve,
            },
        ))
    } else {
        Ok(())
    }
}

/// I9 : les compteurs d'abord, avant tout travail proportionnel au contenu.
pub fn check_bounds(file: &FicheFile, settings: &SettingsFile) -> Result<(), FicheRefusee> {
    let at_settings = |quoi: &'static str, limite: usize, trouve: usize| {
        if trouve > limite {
            Err(refuse(
                Fichier::Settings,
                "",
                Motif::BorneDepassee {
                    quoi,
                    limite,
                    trouve,
                },
            ))
        } else {
            Ok(())
        }
    };
    at_settings("réglages", MAX_SETTINGS, settings.setting.len())?;
    at_settings("réglages retirés", MAX_RETIRED, settings.retired.len())?;
    at_settings("thèmes", MAX_THEMES, settings.theme.len())?;
    at_settings(
        "listes de choix",
        MAX_CHOICE_SETS,
        settings.choice_set.len(),
    )?;
    at_settings("préréglages", MAX_PRESETS, settings.preset.len())?;
    bound("ports", MAX_PORTS, file.port.len())?;
    bound("dossiers", MAX_FOLDERS, file.folder.len())?;
    bound("canaux", MAX_CHANNELS, file.channel.len())?;
    bound("variantes", MAX_VARIANTS, file.variant.len())?;
    bound("variables d'environnement", MAX_ENV, file.env.len())?;
    bound("secrets générés", MAX_SECRETS, file.secret.len())?;
    bound("contrats", MAX_AGREEMENTS, file.agreement.len())?;
    bound(
        "réglages non exposés",
        MAX_NOT_EXPOSED,
        file.not_exposed.len(),
    )?;
    bound("contrôles croisés", MAX_CHECKS, file.check.len())?;
    bound("migrations", MAX_MIGRATIONS, file.migration.len())?;
    for migration in &file.migration {
        bound(
            "opérations d'une migration",
            MAX_MIGRATION_OPS,
            migration.ops.len(),
        )?;
    }
    bound(
        "téléchargements du premier démarrage",
        MAX_DOWNLOADS,
        file.first_start.download.len(),
    )?;
    bound(
        "étapes du premier démarrage",
        MAX_STAGES,
        file.first_start.stage.len(),
    )?;
    if let Some(sequence) = &file.stop.sequence {
        bound("étapes d'arrêt", MAX_STOP_STEPS, sequence.step.len())?;
    }
    if let Some(save) = &file.save {
        bound("étapes d'enregistrement", MAX_STOP_STEPS, save.step.len())?;
    }
    if let Some(worlds) = &file.worlds {
        bound(
            "fichiers compagnons d'un monde",
            MAX_COMPANIONS,
            worlds.companions.len(),
        )?;
        bound("chemins liés d'un monde", MAX_COMPANIONS, worlds.also.len())?;
    }
    Ok(())
}

// ---- Adresses (I5), repères (I6) --------------------------------------------------------------------

/// I5 : une adresse de fiche est affichée, jamais contactée, et en `https`.
pub fn check_url(fichier: Fichier, at: &str, url: &str) -> Result<(), FicheRefusee> {
    match https_url_ok(url) {
        UrlCheck::Ok => Ok(()),
        UrlCheck::NotHttps => Err(refuse(fichier, at, Motif::AdresseNonHttps)),
        UrlCheck::Invalid => Err(refuse(fichier, at, Motif::AdresseInvalide)),
    }
}

/// I6 : un repère de journal est une sous-chaîne littérale d'une ligne, bornée.
fn check_literal(at: &str, quoi: &'static str, text: &str) -> Result<(), FicheRefusee> {
    if literal_ok(text, MAX_LITERAL_CHARS) {
        Ok(())
    } else {
        Err(refuse(
            Fichier::Fiche,
            at,
            Motif::RepereInvalide { raison: quoi },
        ))
    }
}

// ---- Valeurs dérivées (I3) --------------------------------------------------------------------------

/// `derive` : un préfixe littéral, la valeur d'UN réglage, un suffixe littéral. Le réglage source est un
/// texte à jeu de caractères fermé : il borne le résultat.
pub fn check_derive(
    fichier: Fichier,
    at: &str,
    derive: &Derive,
    settings: &[Setting],
) -> Result<(), FicheRefusee> {
    for literal in [&derive.prefix, &derive.suffix].into_iter().flatten() {
        if literal.chars().count() > MAX_LITERAL_CHARS || literal.chars().any(is_risky_char) {
            return Err(refuse(
                fichier,
                at,
                Motif::DeriveInvalide {
                    raison: "préfixe ou suffixe illisible ou trop long",
                },
            ));
        }
    }
    let Some(source) = settings.iter().find(|s| s.id == derive.from) else {
        return Err(refuse(
            fichier,
            at,
            Motif::ReferenceInconnue {
                genre: "réglage source d'une valeur dérivée",
                id: derive.from.clone(),
            },
        ));
    };
    match source.spec {
        ValueSpec::Text {
            charset: Charset::FileStem | Charset::ArgSafe,
            ..
        } => Ok(()),
        _ => Err(refuse(
            fichier,
            at,
            Motif::DeriveInvalide {
                raison: "le réglage source est un texte à jeu de caractères fermé (nom de fichier ou argument)",
            },
        )),
    }
}

/// Le réglage CIBLE d'une valeur dérivée est un texte, et le préfixe et le suffixe de la fiche tiennent dans
/// SON jeu de caractères (I3) : sur le canal des arguments, c'est l'alphabet fermé `arg_safe` (ni espace, ni
/// `=`, ni `/`, ni `$`, ni tiret ou plus en tête), pour que le texte d'une fiche ne devienne pas un
/// argument libre.
pub fn check_derive_target(
    fichier: Fichier,
    at: &str,
    derive: &Derive,
    target: &Setting,
) -> Result<(), FicheRefusee> {
    let ValueSpec::Text { charset, .. } = target.spec else {
        return Err(refuse(
            fichier,
            at,
            Motif::DeriveInvalide {
                raison: "la cible d'une valeur dérivée est un réglage de type texte",
            },
        ));
    };
    for literal in [&derive.prefix, &derive.suffix].into_iter().flatten() {
        if first_refused_char(charset, literal).is_some() {
            return Err(refuse(
                fichier,
                at,
                Motif::DeriveInvalide {
                    raison: "préfixe ou suffixe hors du jeu de caractères du réglage cible",
                },
            ));
        }
    }
    Ok(())
}

// ---- Étapes (I4) ---------------------------------------------------------------------------------------

/// Vérifie une suite d'étapes et rend sa durée au pire, en secondes.
fn check_steps(at: &str, steps: &[Step], allow_quit: bool) -> Result<u32, FicheRefusee> {
    let mut total = 0u32;
    for (index, step) in steps.iter().enumerate() {
        let here = format!("{at}.step[{index}]");
        let forbidden = |fields: &[(&str, bool)], kind: &str| -> Result<(), FicheRefusee> {
            for (name, present) in fields {
                if *present {
                    return Err(refuse(
                        Fichier::Fiche,
                        here.as_str(),
                        Motif::ChampIncoherent {
                            champ: (*name).into(),
                            avec: format!("pour une étape {kind}"),
                        },
                    ));
                }
            }
            Ok(())
        };
        let need = |present: bool, champ: &str| -> Result<(), FicheRefusee> {
            if present {
                Ok(())
            } else {
                Err(refuse(
                    Fichier::Fiche,
                    here.as_str(),
                    Motif::ChampManquant {
                        champ: champ.into(),
                    },
                ))
            }
        };
        match step.kind {
            StepKind::Unknown => {
                return Err(refuse(
                    Fichier::Fiche,
                    here.as_str(),
                    Motif::GenreDEtapeInconnu,
                ));
            }
            StepKind::StdinLine => {
                forbidden(
                    &[
                        ("contains", step.contains.is_some()),
                        ("timeout_s", step.timeout_s.is_some()),
                        ("required", step.required.is_some()),
                        ("seconds", step.seconds.is_some()),
                    ],
                    "stdin_line",
                )?;
                need(step.line.is_some(), "line")?;
                need(step.quits.is_some(), "quits")?;
                if let Some(line) = &step.line
                    && let Some(raison) = order_refusal(line)
                {
                    return Err(refuse(
                        Fichier::Fiche,
                        here.as_str(),
                        Motif::OrdreInvalide { raison },
                    ));
                }
                if step.quits == Some(true) {
                    if !allow_quit {
                        return Err(refuse(
                            Fichier::Fiche,
                            here.as_str(),
                            Motif::SequenceIncoherente {
                                raison: "un enregistrement à chaud ne quitte jamais le jeu",
                            },
                        ));
                    }
                    if index + 1 != steps.len() {
                        return Err(refuse(
                            Fichier::Fiche,
                            here.as_str(),
                            Motif::SequenceIncoherente {
                                raison: "étape après l'ordre de sortie",
                            },
                        ));
                    }
                }
            }
            StepKind::WaitLog => {
                forbidden(
                    &[
                        ("line", step.line.is_some()),
                        ("quits", step.quits.is_some()),
                        ("seconds", step.seconds.is_some()),
                    ],
                    "wait_log",
                )?;
                need(step.contains.is_some(), "contains")?;
                need(step.timeout_s.is_some(), "timeout_s")?;
                need(step.required.is_some(), "required")?;
                if let Some(contains) = &step.contains {
                    check_literal(&here, "signe de journal d'une étape d'attente", contains)?;
                }
                let timeout = step.timeout_s.unwrap_or(0);
                if timeout == 0 || timeout > MAX_WAIT_LOG_SECONDS {
                    return Err(refuse(
                        Fichier::Fiche,
                        here.as_str(),
                        Motif::BorneInvalide {
                            quoi: "`timeout_s` d'un wait_log : de 1 à 300 secondes",
                        },
                    ));
                }
                total += timeout;
            }
            StepKind::Wait => {
                forbidden(
                    &[
                        ("line", step.line.is_some()),
                        ("quits", step.quits.is_some()),
                        ("contains", step.contains.is_some()),
                        ("timeout_s", step.timeout_s.is_some()),
                        ("required", step.required.is_some()),
                    ],
                    "wait",
                )?;
                need(step.seconds.is_some(), "seconds")?;
                let seconds = step.seconds.unwrap_or(0);
                if seconds == 0 || seconds > MAX_WAIT_SECONDS {
                    return Err(refuse(
                        Fichier::Fiche,
                        here.as_str(),
                        Motif::BorneInvalide {
                            quoi: "`seconds` d'un wait : de 1 à 120 secondes",
                        },
                    ));
                }
                total += seconds;
            }
        }
    }
    Ok(total)
}

// ---- Noms -------------------------------------------------------------------------------------------------

fn env_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    name.chars().count() <= 64
        && (first.is_ascii_uppercase() || first == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn ident(at: &str, ou: &'static str, id: &str) -> Result<(), FicheRefusee> {
    if ident_ok(id, 1, 32, true) {
        Ok(())
    } else {
        Err(refuse(
            Fichier::Fiche,
            at,
            Motif::IdentifiantInvalide { ou },
        ))
    }
}

fn unique<'a>(genre: &'static str, ids: impl Iterator<Item = &'a str>) -> Result<(), FicheRefusee> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(refuse(
                Fichier::Fiche,
                "",
                Motif::IdentifiantEnDouble {
                    genre,
                    id: id.chars().take(48).collect(),
                },
            ));
        }
    }
    Ok(())
}

fn repository_ok(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && !text.starts_with('/')
        && !text.ends_with('/')
        && !text.contains("//")
        && text.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-' | '/')
        })
}

fn tag_ok(text: &str) -> Result<(), Motif> {
    if text.is_empty() || text.eq_ignore_ascii_case("latest") {
        return Err(Motif::ImageSansVersion);
    }
    if text.len() > 128
        || !text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(Motif::ImageSansVersion);
    }
    Ok(())
}

fn digest_ok(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn path_motif(refused: PathRefused, ou: &str) -> Motif {
    let _ = refused;
    Motif::CheminInvalide { ou: ou.to_owned() }
}

fn channel_kind_name(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Env => "env",
        ChannelKind::Args => "args",
        ChannelKind::File => "file",
        ChannelKind::Unknown => "inconnu",
    }
}

// ---- fiche.toml : I1 à I8 ---------------------------------------------------------------------------

pub fn check_fiche_file(file: &FicheFile) -> Result<(), FicheRefusee> {
    check_identity(file)?;
    check_image(file)?;
    check_variants(file)?;
    check_folders(file)?;
    check_ports(file)?;
    check_channels(file)?;
    check_env_and_secrets(file)?;
    check_remote_console(file)?;
    check_agreements(file)?;
    check_first_start(file)?;
    check_stop(file)?;
    check_resources(file)?;
    check_worlds_shape(file)?;
    Ok(())
}

fn check_identity(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    if !ident_ok(&file.id, MIN_FICHE_ID_CHARS, MAX_FICHE_ID_CHARS, false) {
        return Err(refuse(f, "id", Motif::IdentifiantInvalide { ou: "fiche" }));
    }
    if !ident_ok(&file.family, 1, MAX_FICHE_ID_CHARS, false) {
        return Err(refuse(
            f,
            "family",
            Motif::IdentifiantInvalide { ou: "famille" },
        ));
    }
    if file.version == 0 {
        return Err(refuse(
            f,
            "version",
            Motif::BorneInvalide {
                quoi: "la version d'une fiche est au moins 1",
            },
        ));
    }
    check_text(f, "name", "nom", &file.name, MAX_NAME_CHARS)?;
    check_text(f, "summary", "résumé", &file.summary, MAX_SUMMARY_CHARS)?;
    if let super::schema::fiche::Platform::Unavailable(u) = &file.platform {
        check_text(
            f,
            "platform",
            "raison d'indisponibilité",
            &u.unavailable,
            MAX_SUMMARY_CHARS,
        )?;
    }
    Ok(())
}

fn check_image(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let image = &file.image;
    if !repository_ok(&image.repository) {
        return Err(refuse(f, "image.repository", Motif::DepotInvalide));
    }
    tag_ok(&image.tag).map_err(|m| refuse(f, "image.tag", m))?;
    if let Some(digest) = &image.digest
        && !digest_ok(digest)
    {
        return Err(refuse(f, "image.digest", Motif::EmpreinteInvalide));
    }
    if file.status == Status::Published && image.digest.is_none() {
        return Err(refuse(f, "image.digest", Motif::PublieeSansEmpreinte));
    }
    Ok(())
}

fn check_variants(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("variante", file.variant.iter().map(|v| v.id.as_str()))?;
    for (i, variant) in file.variant.iter().enumerate() {
        let at = format!("variant[{i}]");
        ident(&at, "variante", &variant.id)?;
        check_text(
            f,
            &at,
            "libellé de variante",
            &variant.label,
            MAX_LABEL_CHARS,
        )?;
        check_text(f, &at, "aide de variante", &variant.help, MAX_HELP_CHARS)?;
        if let Some(tag) = &variant.image_tag {
            tag_ok(tag).map_err(|m| refuse(f, format!("{at}.image_tag"), m))?;
        }
        if let Some(digest) = &variant.image_digest
            && !digest_ok(digest)
        {
            return Err(refuse(
                f,
                format!("{at}.image_digest"),
                Motif::EmpreinteInvalide,
            ));
        }
        if file.status == Status::Published
            && variant.image_tag.is_some()
            && variant.image_digest.is_none()
        {
            return Err(refuse(
                f,
                format!("{at}.image_digest"),
                Motif::PublieeSansEmpreinte,
            ));
        }
        for (j, env) in variant.env.iter().enumerate() {
            let here = format!("{at}.env[{j}]");
            if !env_name_ok(&env.name) {
                return Err(refuse(
                    f,
                    here,
                    Motif::IdentifiantInvalide {
                        ou: "nom de variable",
                    },
                ));
            }
            fixed_value_ok(&here, &env.value)?;
        }
    }
    if !file.variant.is_empty() && file.variant.iter().filter(|v| v.default).count() != 1 {
        return Err(refuse(
            f,
            "variant",
            Motif::ChampIncoherent {
                champ: "default".into(),
                avec: "(exactement une variante par défaut)".into(),
            },
        ));
    }
    Ok(())
}

fn fixed_value_ok(at: &str, value: &str) -> Result<(), FicheRefusee> {
    if value.chars().count() > MAX_ENV_VALUE_CHARS {
        return Err(refuse(
            Fichier::Fiche,
            at,
            Motif::ValeurFixeInvalide {
                raison: "plus de 256 caractères",
            },
        ));
    }
    if value.chars().any(is_risky_char) {
        return Err(refuse(
            Fichier::Fiche,
            at,
            Motif::ValeurFixeInvalide {
                raison: "caractère de contrôle ou invisible",
            },
        ));
    }
    Ok(())
}

fn check_folders(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("dossier", file.folder.iter().map(|d| d.id.as_str()))?;
    let mut targets = BTreeSet::new();
    for (i, folder) in file.folder.iter().enumerate() {
        let at = format!("folder[{i}]");
        ident(&at, "dossier", &folder.id)?;
        if !container_target_ok(&folder.target) {
            return Err(refuse(
                f,
                at,
                Motif::CheminInvalide {
                    ou: "cible du dossier dans le service".into(),
                },
            ));
        }
        let normalized = folder.target.trim_end_matches('/').to_owned();
        if !targets.insert(normalized) {
            return Err(refuse(f, at, Motif::CibleEnDouble));
        }
        if let Some(hint) = &folder.drop_hint {
            check_text(f, &at, "indication de dépôt", hint, MAX_HELP_CHARS)?;
        }
        if let Some(only) = &folder.only_variants {
            check_variant_refs(file, f, &at, only)?;
        }
    }
    Ok(())
}

fn check_ports(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("port", file.port.iter().map(|p| p.id.as_str()))?;
    let mut seen = BTreeSet::new();
    for (i, port) in file.port.iter().enumerate() {
        let at = format!("port[{i}]");
        ident(&at, "port", &port.id)?;
        if port.container < MIN_PORT {
            return Err(refuse(
                f,
                at,
                Motif::PortReserve {
                    port: port.container,
                },
            ));
        }
        if let Some(host) = port.default_host
            && host < MIN_PORT
        {
            return Err(refuse(f, at, Motif::PortReserve { port: host }));
        }
        if !seen.insert((port.container, port.protocol as u8)) {
            return Err(refuse(
                f,
                at,
                Motif::PortEnDouble {
                    port: port.container,
                },
            ));
        }
        if let Some(group) = &port.group {
            ident(&at, "groupe de ports", group)?;
        }
        if port.role == PortRole::RemoteConsole && port.publish != Publish::Never {
            return Err(refuse(f, at, Motif::ConsolePubliee));
        }
        if let Some(only) = &port.only_variants {
            check_variant_refs(file, f, &at, only)?;
        }
    }
    Ok(())
}

fn check_channels(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("canal", file.channel.iter().map(|c| c.id.as_str()))?;
    let mut files_seen = BTreeSet::new();
    for (i, channel) in file.channel.iter().enumerate() {
        let at = format!("channel[{i}]");
        ident(&at, "canal", &channel.id)?;
        if let Some(only) = &channel.only_variants {
            check_variant_refs(file, f, &at, only)?;
        }
        match channel.kind {
            ChannelKind::Unknown => return Err(refuse(f, at, Motif::GenreDeCanalInconnu)),
            ChannelKind::Env | ChannelKind::Args => {
                for (name, present) in [
                    ("format", channel.format.is_some()),
                    ("folder", channel.folder.is_some()),
                    ("path", channel.path.is_some()),
                    ("mode", channel.mode.is_some()),
                    ("written_by_game", channel.written_by_game.is_some()),
                    ("raw_edit", channel.raw_edit.is_some()),
                    ("kv", channel.kv.is_some()),
                    ("xml", channel.xml.is_some()),
                ] {
                    if present {
                        return Err(refuse(
                            f,
                            at,
                            Motif::ChampIncoherent {
                                champ: name.into(),
                                avec: format!(
                                    "pour un canal de genre {}",
                                    channel_kind_name(channel.kind)
                                ),
                            },
                        ));
                    }
                }
                let same_kind = file
                    .channel
                    .iter()
                    .filter(|c| c.kind == channel.kind)
                    .count();
                if same_kind > 1 {
                    return Err(refuse(
                        f,
                        at,
                        Motif::IdentifiantEnDouble {
                            genre: "canal de ce genre",
                            id: channel_kind_name(channel.kind).into(),
                        },
                    ));
                }
            }
            ChannelKind::File => check_file_channel(file, channel, &at, &mut files_seen)?,
        }
    }
    Ok(())
}

fn check_file_channel(
    file: &FicheFile,
    channel: &Channel,
    at: &str,
    files_seen: &mut BTreeSet<(String, String)>,
) -> Result<(), FicheRefusee> {
    use super::schema::fiche::{FileMode, WrittenByGame};
    use super::value::FileFormat;

    let f = Fichier::Fiche;
    let miss = |champ: &str| {
        refuse(
            f,
            at,
            Motif::ChampManquant {
                champ: champ.into(),
            },
        )
    };
    let format = channel.format.ok_or_else(|| miss("format"))?;
    let folder = channel.folder.as_ref().ok_or_else(|| miss("folder"))?;
    let path = channel.path.as_ref().ok_or_else(|| miss("path"))?;
    let _: FileMode = channel.mode.ok_or_else(|| miss("mode"))?;
    let _: WrittenByGame = channel
        .written_by_game
        .ok_or_else(|| miss("written_by_game"))?;
    channel.raw_edit.ok_or_else(|| miss("raw_edit"))?;
    if !file.folder.iter().any(|d| &d.id == folder) {
        return Err(refuse(f, at, Motif::DossierInconnu { id: folder.clone() }));
    }
    if let Err(refused) = ChannelPath::new(path) {
        return Err(refuse(
            f,
            at,
            path_motif(refused, "chemin du fichier du canal"),
        ));
    }
    if !files_seen.insert((folder.clone(), path.clone())) {
        return Err(refuse(
            f,
            at,
            Motif::IdentifiantEnDouble {
                genre: "fichier de canal",
                id: path.chars().take(48).collect(),
            },
        ));
    }
    let single_ascii = |text: &str| {
        let mut chars = text.chars();
        matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_punctuation())
    };
    match format {
        FileFormat::KvLines => {
            if channel.xml.is_some() {
                return Err(refuse(
                    f,
                    at,
                    Motif::ChampIncoherent {
                        champ: "xml".into(),
                        avec: "pour le format kv_lines".into(),
                    },
                ));
            }
            let kv = channel.kv.as_ref().ok_or_else(|| miss("kv"))?;
            if !single_ascii(&kv.separator)
                || !single_ascii(&kv.comment)
                || kv.separator == kv.comment
            {
                return Err(refuse(
                    f,
                    at,
                    Motif::BorneInvalide {
                        quoi: "séparateur et marque de commentaire : un signe de ponctuation ASCII chacun, différents",
                    },
                ));
            }
        }
        FileFormat::XmlProperties => {
            if channel.kv.is_some() {
                return Err(refuse(
                    f,
                    at,
                    Motif::ChampIncoherent {
                        champ: "kv".into(),
                        avec: "pour le format xml_properties".into(),
                    },
                ));
            }
            let xml = channel.xml.as_ref().ok_or_else(|| miss("xml"))?;
            let name_ok = |n: &str| {
                let mut chars = n.chars();
                chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && n.len() <= 64
                    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            };
            if ![&xml.root, &xml.element, &xml.key_attr, &xml.value_attr]
                .iter()
                .all(|n| name_ok(n))
                || xml.key_attr == xml.value_attr
            {
                return Err(refuse(
                    f,
                    at,
                    Motif::BorneInvalide {
                        quoi: "noms d'élément et d'attributs XML : lettres, chiffres, `_`, `-`, `.`",
                    },
                ));
            }
        }
    }
    Ok(())
}

fn check_env_and_secrets(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("secret généré", file.secret.iter().map(|s| s.id.as_str()))?;
    for (i, secret) in file.secret.iter().enumerate() {
        let at = format!("secret[{i}]");
        ident(&at, "secret généré", &secret.id)?;
        if secret.length < MIN_GENERATED_SECRET_LENGTH
            || secret.length > MAX_GENERATED_SECRET_LENGTH
        {
            return Err(refuse(
                f,
                at,
                Motif::BorneInvalide {
                    quoi: "longueur d'un secret généré : de 24 à 64",
                },
            ));
        }
    }
    for (i, env) in file.env.iter().enumerate() {
        let at = format!("env[{i}]");
        if !env_name_ok(&env.name) {
            return Err(refuse(
                f,
                at,
                Motif::IdentifiantInvalide {
                    ou: "nom de variable",
                },
            ));
        }
        let origins = [
            env.value.is_some(),
            env.derive.is_some(),
            env.secret.is_some(),
            env.owner.is_some(),
        ]
        .iter()
        .filter(|p| **p)
        .count();
        if origins != 1 {
            return Err(refuse(
                f,
                at,
                Motif::ChampIncoherent {
                    champ: "value".into(),
                    avec: "(exactement une de value, derive, secret, owner)".into(),
                },
            ));
        }
        if let Some(value) = &env.value {
            fixed_value_ok(&at, value)?;
        }
        if let Some(secret) = &env.secret
            && !file.secret.iter().any(|s| &s.id == secret)
        {
            return Err(refuse(
                f,
                at,
                Motif::ReferenceInconnue {
                    genre: "secret généré",
                    id: secret.clone(),
                },
            ));
        }
    }
    Ok(())
}

/// I7 (forme) : la console déclarée désigne un port de rôle `remote_console` et un secret généré.
fn check_remote_console(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let has_console_port = file.port.iter().any(|p| p.role == PortRole::RemoteConsole);
    let Some(console) = &file.remote_console else {
        if has_console_port {
            return Err(refuse(
                f,
                "port",
                Motif::ConsoleIncoherente {
                    raison: "un port de console distante exige la déclaration `[remote_console]`",
                },
            ));
        }
        return Ok(());
    };
    let Some(port) = file.port.iter().find(|p| p.id == console.port) else {
        return Err(refuse(
            f,
            "remote_console.port",
            Motif::ReferenceInconnue {
                genre: "port",
                id: console.port.clone(),
            },
        ));
    };
    if port.role != PortRole::RemoteConsole {
        return Err(refuse(
            f,
            "remote_console.port",
            Motif::ConsoleIncoherente {
                raison: "le port désigné n'a pas le rôle `remote_console`",
            },
        ));
    }
    if port.publish != Publish::Never {
        return Err(refuse(f, "remote_console.port", Motif::ConsolePubliee));
    }
    if !file.secret.iter().any(|s| s.id == console.password) {
        return Err(refuse(
            f,
            "remote_console.password",
            Motif::ConsoleMotDePasseNonGenere,
        ));
    }
    Ok(())
}

fn check_agreements(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    unique("contrat", file.agreement.iter().map(|a| a.id.as_str()))?;
    for (i, agreement) in file.agreement.iter().enumerate() {
        let at = format!("agreement[{i}]");
        ident(&at, "contrat", &agreement.id)?;
        check_text(
            f,
            &at,
            "titre de contrat",
            &agreement.title,
            MAX_LABEL_CHARS,
        )?;
        check_text(
            f,
            &at,
            "phrase de contrat",
            &agreement.sentence,
            MAX_HELP_CHARS,
        )?;
        check_url(f, &at, &agreement.url)?;
        let binds = &agreement.binds;
        let Some(channel) = file.channel.iter().find(|c| c.id == binds.channel) else {
            return Err(refuse(
                f,
                at,
                Motif::ReferenceInconnue {
                    genre: "canal",
                    id: binds.channel.clone(),
                },
            ));
        };
        // Seule une variable d'environnement porte la valeur d'acceptation : ni un argument, ni un fichier (la
        // traduction ne l'écrit pas dans un fichier ; l'accepter sans l'écrire ferait échouer le jeu en silence).
        if channel.kind != ChannelKind::Env {
            return Err(refuse(
                f,
                at,
                Motif::ChampIncoherent {
                    champ: "binds.channel".into(),
                    avec: "pour un canal env seulement (un contrat ne passe ni en argument ni dans un fichier)".into(),
                },
            ));
        }
        if !literal_ok(&binds.key, MAX_LITERAL_CHARS)
            || !literal_ok(&binds.accepted_value, MAX_LITERAL_CHARS)
        {
            return Err(refuse(
                f,
                at,
                Motif::ValeurFixeInvalide {
                    raison: "clé ou valeur d'acceptation illisible",
                },
            ));
        }
        // I8 : la valeur d'acceptation n'a QU'UN chemin, le geste de l'utilisateur.
        if channel.kind == ChannelKind::Env {
            if file.env.iter().any(|e| e.name == binds.key) {
                return Err(refuse(
                    f,
                    at,
                    Motif::ContratAccepteDAvance {
                        ou: "[[env]] fixe".into(),
                    },
                ));
            }
            if file
                .variant
                .iter()
                .any(|v| v.env.iter().any(|e| e.name == binds.key))
            {
                return Err(refuse(
                    f,
                    at,
                    Motif::ContratAccepteDAvance {
                        ou: "variable fixe d'une variante".into(),
                    },
                ));
            }
        }
    }
    Ok(())
}

fn check_first_start(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let first = &file.first_start;
    if first.ready_timeout_s < MIN_READY_TIMEOUT_SECONDS
        || first.ready_timeout_s > MAX_READY_TIMEOUT_SECONDS
    {
        return Err(refuse(
            f,
            "first_start.ready_timeout_s",
            Motif::BorneInvalide {
                quoi: "délai « prêt » : de 60 secondes à 6 heures",
            },
        ));
    }
    for (i, download) in first.download.iter().enumerate() {
        let at = format!("first_start.download[{i}]");
        check_text(f, &at, "objet téléchargé", &download.what, MAX_LABEL_CHARS)?;
        check_text(
            f,
            &at,
            "source du téléchargement",
            &download.source_label,
            MAX_LABEL_CHARS,
        )?;
        if download.bytes.is_some() && download.evidence.is_unmeasured() {
            return Err(refuse(
                f,
                at,
                Motif::RessourceNonMesureeAvecNombre {
                    ressource: "téléchargement du premier démarrage",
                },
            ));
        }
        if let Evidence::Publisher(p) = &download.evidence {
            check_url(f, &at, &p.publisher)?;
        }
    }
    for (i, stage) in first.stage.iter().enumerate() {
        let at = format!("first_start.stage[{i}]");
        check_text(f, &at, "libellé d'étape", &stage.label, MAX_LABEL_CHARS)?;
        check_literal(
            &at,
            "repère d'étape du premier démarrage",
            &stage.log_contains,
        )?;
    }
    if let Some(ready) = &file.ready {
        check_literal("ready", "signe « prêt »", &ready.log_contains)?;
    }
    if let Some(version) = &file.game_version {
        if !literal_ok(&version.after, 100) {
            return Err(refuse(
                f,
                "game_version.after",
                Motif::RepereInvalide {
                    raison: "repère de début de la version du jeu illisible",
                },
            ));
        }
        if version.until_any.is_empty() || version.until_any.len() > 8 {
            return Err(refuse(
                f,
                "game_version.until_any",
                Motif::RepereInvalide {
                    raison: "de un à huit repères de fin",
                },
            ));
        }
        for until in &version.until_any {
            let mut chars = until.chars();
            let one = matches!((chars.next(), chars.next()), (Some(c), None) if !is_risky_char(c) || c == '\t');
            if !one {
                return Err(refuse(
                    f,
                    "game_version.until_any",
                    Motif::RepereInvalide {
                        raison: "un seul caractère par repère de fin de la version du jeu",
                    },
                ));
            }
        }
    }
    if let Some(worlds) = &file.worlds
        && let Some(loaded) = &worlds.loaded_contains
    {
        check_literal("worlds.loaded_contains", "signe de monde chargé", loaded)?;
    }
    Ok(())
}

/// Un signe « prêt » ou une séquence `untested` n'est jamais exécuté : ce contrôle ne regarde que la forme.
fn check_stop(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let stop = &file.stop;
    if stop.grace_s < MIN_GRACE_SECONDS || stop.grace_s > MAX_GRACE_SECONDS {
        return Err(refuse(
            f,
            "stop.grace_s",
            Motif::BorneInvalide {
                quoi: "délai d'arrêt : de 10 à 600 secondes",
            },
        ));
    }
    match (&stop.world_saved, &stop.sequence) {
        (WorldSaved::BySequence, None) => {
            return Err(refuse(
                f,
                "stop",
                Motif::SequenceIncoherente {
                    raison: "`by_sequence` sans `[stop.sequence]`",
                },
            ));
        }
        (WorldSaved::BySequence, Some(sequence)) if sequence.step.is_empty() => {
            return Err(refuse(
                f,
                "stop.sequence",
                Motif::SequenceIncoherente {
                    raison: "séquence sans étape",
                },
            ));
        }
        (WorldSaved::ByImageOnSignal | WorldSaved::NotSaved, Some(_)) => {
            return Err(refuse(
                f,
                "stop.sequence",
                Motif::SequenceIncoherente {
                    raison: "une séquence n'a de sens qu'avec `by_sequence`",
                },
            ));
        }
        _ => {}
    }
    let mut total = 0u32;
    if let Some(sequence) = &stop.sequence {
        total += check_steps("stop.sequence", &sequence.step, true)?;
    }
    if total > MAX_SEQUENCE_SECONDS {
        return Err(refuse(
            f,
            "stop.sequence",
            Motif::BorneDepassee {
                quoi: "durée de la séquence d'arrêt (secondes)",
                limite: MAX_SEQUENCE_SECONDS as usize,
                trouve: total as usize,
            },
        ));
    }
    if let Some(save) = &file.save {
        let seconds = check_steps("save", &save.step, false)?;
        if seconds > MAX_SEQUENCE_SECONDS {
            return Err(refuse(
                f,
                "save",
                Motif::BorneDepassee {
                    quoi: "durée de la séquence d'enregistrement (secondes)",
                    limite: MAX_SEQUENCE_SECONDS as usize,
                    trouve: seconds as usize,
                },
            ));
        }
        if save.status == Proof::Tested && save.step.is_empty() {
            return Err(refuse(
                f,
                "save",
                Motif::SequenceIncoherente {
                    raison: "séquence d'enregistrement sans étape",
                },
            ));
        }
    }
    Ok(())
}

fn check_resources(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let r = &file.resources;
    for (name, evidence, has_number) in [
        (
            "cpu",
            &r.cpu.evidence,
            r.cpu.min_millis.is_some() || r.cpu.recommended_millis.is_some(),
        ),
        (
            "memory",
            &r.memory.evidence,
            r.memory.min_bytes.is_some() || r.memory.recommended_bytes.is_some(),
        ),
        ("disk", &r.disk.evidence, r.disk.install_bytes.is_some()),
    ] {
        if evidence.is_unmeasured() && has_number {
            return Err(refuse(
                f,
                format!("resources.{name}"),
                Motif::RessourceNonMesureeAvecNombre { ressource: name },
            ));
        }
        if let Evidence::Publisher(p) = evidence {
            check_url(f, &format!("resources.{name}"), &p.publisher)?;
        }
    }
    if let (Some(min), Some(rec)) = (r.memory.min_bytes, r.memory.recommended_bytes)
        && min > rec
    {
        return Err(refuse(
            f,
            "resources.memory",
            Motif::BorneInvalide {
                quoi: "mémoire minimale supérieure à la recommandée",
            },
        ));
    }
    if let (Some(min), Some(rec)) = (r.cpu.min_millis, r.cpu.recommended_millis)
        && min > rec
    {
        return Err(refuse(
            f,
            "resources.cpu",
            Motif::BorneInvalide {
                quoi: "processeur minimal supérieur au recommandé",
            },
        ));
    }
    check_memory_tiers(file)?;
    if file.status == Status::Published
        && let Some(gap) = recommended_gap(file)
    {
        return Err(refuse(
            f,
            "resources.memory",
            Motif::PublieeSansMemoireRecommandee {
                raison: gap.explication(),
            },
        ));
    }
    Ok(())
}

/// HRT-122 : les crans déclarés (BR-RES-032, BR-RES-033) et la liste des crans cachés.
fn check_memory_tiers(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let r = &file.resources;
    let mut seen: Vec<TierId> = Vec::new();
    let mut floor = r.memory.recommended_bytes;
    let mut declared: Vec<(TierId, &MemoryTier)> = Vec::new();
    for (i, tier) in r.memory_tier.iter().enumerate() {
        let at = format!("resources.memory_tier[{i}]");
        let Some(id) = TierId::parse(&tier.id) else {
            return Err(refuse(
                f,
                at,
                Motif::CranInconnu {
                    id: tier.id.clone(),
                },
            ));
        };
        if !id.is_declarable() {
            return Err(refuse(
                f,
                at,
                Motif::CranNonDeclarable {
                    id: tier.id.clone(),
                },
            ));
        }
        if seen.contains(&id) {
            return Err(refuse(
                f,
                at,
                Motif::CranEnDouble {
                    id: tier.id.clone(),
                },
            ));
        }
        seen.push(id);
        declared.push((id, tier));
    }
    // Strictement croissants dans l'ordre des crans, et tous au-dessus de la mémoire recommandée.
    declared.sort_by_key(|(id, _)| *id);
    for (_, tier) in declared {
        let at = format!("resources.memory_tier[{}]", tier.id);
        match &tier.evidence {
            Evidence::Plain(PlainEvidence::Measured) => {}
            Evidence::Plain(_) => {
                return Err(refuse(
                    f,
                    at,
                    Motif::CranPreuveInterdite {
                        id: tier.id.clone(),
                    },
                ));
            }
            Evidence::Publisher(p) => check_url(f, &at, &p.publisher)?,
        }
        let Some(below) = floor else {
            return Err(refuse(
                f,
                at,
                Motif::CranBorneInvalide {
                    id: tier.id.clone(),
                    raison: "la fiche n'a pas de mémoire recommandée sous laquelle placer ce cran",
                },
            ));
        };
        if tier.from_bytes <= below {
            return Err(refuse(
                f,
                at,
                Motif::CranBorneInvalide {
                    id: tier.id.clone(),
                    raison: "`from_bytes` doit être au-dessus de la mémoire recommandée et des crans précédents",
                },
            ));
        }
        floor = Some(tier.from_bytes);
    }
    if let Some(tiers) = &r.tiers {
        let mut hidden: Vec<TierId> = Vec::new();
        for id in &tiers.hidden {
            let at = "resources.tiers.hidden".to_string();
            let Some(parsed) = TierId::parse(id) else {
                return Err(refuse(f, at, Motif::CranInconnu { id: id.clone() }));
            };
            if hidden.contains(&parsed) {
                return Err(refuse(f, at, Motif::CranEnDouble { id: id.clone() }));
            }
            hidden.push(parsed);
        }
    }
    Ok(())
}

fn check_worlds_shape(file: &FicheFile) -> Result<(), FicheRefusee> {
    let f = Fichier::Fiche;
    let Some(worlds) = &file.worlds else {
        return Ok(());
    };
    match file.folder.iter().find(|d| d.id == worlds.folder) {
        None => {
            return Err(refuse(
                f,
                "worlds.folder",
                Motif::DossierInconnu {
                    id: worlds.folder.clone(),
                },
            ));
        }
        Some(folder) if folder.role != FolderRole::World => {
            return Err(refuse(
                f,
                "worlds.folder",
                Motif::ChampIncoherent {
                    champ: "folder".into(),
                    avec: "pour un dossier qui n'a pas le rôle `world`".into(),
                },
            ));
        }
        Some(_) => {}
    }
    for suffix in worlds.suffix.iter().chain(worlds.companions.iter()) {
        let ok = suffix.starts_with('.')
            && suffix.len() <= 16
            && suffix.len() > 1
            && suffix[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
        if !ok {
            return Err(refuse(
                f,
                "worlds",
                Motif::CheminInvalide {
                    ou: "suffixe de monde".into(),
                },
            ));
        }
    }
    for also in &worlds.also {
        if ChannelPath::new(also).is_err() {
            return Err(refuse(
                f,
                "worlds.also",
                Motif::CheminInvalide {
                    ou: "chemin lié à un monde".into(),
                },
            ));
        }
    }
    Ok(())
}

// ---- Invariants croisés fiche × réglages -----------------------------------------------------------

pub fn check_cross(fiche: &Fiche) -> Result<(), FicheRefusee> {
    let file = &fiche.file;
    let s = Fichier::Settings;
    let f = Fichier::Fiche;

    for setting in &fiche.settings {
        let at = format!("setting[{}]", setting.id);
        let channel = fiche.channel(&setting.channel);
        let is_args = channel.is_some_and(|c| c.kind == ChannelKind::Args);
        if is_args {
            check_arg_setting(setting, &at)?;
        } else if setting.arg.is_some() {
            return Err(refuse(
                s,
                at,
                Motif::ChampIncoherent {
                    champ: "arg".into(),
                    avec: "pour un réglage d'un canal qui n'est pas args".into(),
                },
            ));
        }
        if channel.is_some_and(|c| c.kind == ChannelKind::Env) && !env_name_ok(&setting.key) {
            return Err(refuse(
                s,
                at,
                Motif::IdentifiantInvalide {
                    ou: "nom de variable d'un réglage",
                },
            ));
        }
        if let Some(imposed) = &setting.imposed
            && imposed.secret.is_some()
            && is_args
        {
            return Err(refuse(s, at, Motif::SecretEnArgument));
        }
        // I8 : aucun réglage n'écrit la clé d'un contrat.
        for agreement in &file.agreement {
            if agreement.binds.channel == setting.channel && agreement.binds.key == setting.key {
                return Err(refuse(
                    s,
                    at,
                    Motif::ContratAccepteDAvance {
                        ou: "réglage".into(),
                    },
                ));
            }
        }
    }
    // Un réglage exposé ne peut pas aussi être « non exposé ».
    for (i, ne) in file.not_exposed.iter().enumerate() {
        let at = format!("not_exposed[{i}]");
        if !file.channel.iter().any(|c| c.id == ne.channel) {
            return Err(refuse(
                f,
                at,
                Motif::ReferenceInconnue {
                    genre: "canal",
                    id: ne.channel.clone(),
                },
            ));
        }
        check_text(
            f,
            &at,
            "raison d'un réglage non exposé",
            &ne.why,
            MAX_WHY_CHARS,
        )?;
        if !literal_ok(&ne.key, MAX_LITERAL_CHARS) {
            return Err(refuse(
                f,
                at,
                Motif::IdentifiantInvalide {
                    ou: "clé d'un réglage non exposé",
                },
            ));
        }
        if fiche
            .settings
            .iter()
            .any(|st| st.channel == ne.channel && st.key == ne.key)
        {
            return Err(refuse(
                f,
                at,
                Motif::CleEnDouble {
                    canal: ne.channel.clone(),
                    cle: ne.key.clone(),
                },
            ));
        }
    }
    // Variables dérivées d'un réglage.
    for (i, env) in file.env.iter().enumerate() {
        if let Some(derive) = &env.derive {
            check_derive(f, &format!("env[{i}]"), derive, &fiche.settings)?;
        }
    }
    // Ports qui annoncent leur numéro.
    for (i, port) in file.port.iter().enumerate() {
        let at = format!("port[{i}]");
        match (port.mapping, &port.game_setting) {
            (super::schema::fiche::PortMapping::SameNumber, None) => {
                return Err(refuse(
                    f,
                    at,
                    Motif::ChampManquant {
                        champ: "game_setting".into(),
                    },
                ));
            }
            (super::schema::fiche::PortMapping::Nat, Some(_)) => {
                return Err(refuse(
                    f,
                    at,
                    Motif::ChampIncoherent {
                        champ: "game_setting".into(),
                        avec: "pour un port `nat`".into(),
                    },
                ));
            }
            (_, Some(id)) if fiche.setting(id).is_none() => {
                return Err(refuse(
                    f,
                    at,
                    Motif::ReferenceInconnue {
                        genre: "réglage de port",
                        id: id.clone(),
                    },
                ));
            }
            _ => {}
        }
    }
    // I7 : le mot de passe de la console est écrit quelque part par la fiche.
    if let Some(console) = &file.remote_console {
        let in_env = file
            .env
            .iter()
            .any(|e| e.secret.as_deref() == Some(console.password.as_str()));
        let in_setting = fiche.settings.iter().any(|st| {
            st.imposed.as_ref().and_then(|i| i.secret.as_deref()) == Some(console.password.as_str())
        });
        if !in_env && !in_setting {
            return Err(refuse(
                f,
                "remote_console.password",
                Motif::ConsoleIncoherente {
                    raison: "aucune variable ni aucun réglage imposé ne porte le mot de passe généré",
                },
            ));
        }
    }
    // Mondes.
    if let Some(worlds) = &file.worlds {
        match fiche.setting(&worlds.names_setting) {
            None => {
                return Err(refuse(
                    f,
                    "worlds.names_setting",
                    Motif::ReferenceInconnue {
                        genre: "réglage",
                        id: worlds.names_setting.clone(),
                    },
                ));
            }
            Some(name) if !matches!(name.spec, ValueSpec::Text { .. }) => {
                return Err(refuse(
                    f,
                    "worlds.names_setting",
                    Motif::ChampIncoherent {
                        champ: "names_setting".into(),
                        avec: "pour un réglage qui n'est pas un texte".into(),
                    },
                ));
            }
            Some(_) => {}
        }
        for id in &worlds.carried_by_world {
            if fiche.setting(id).is_none() {
                return Err(refuse(
                    f,
                    "worlds.carried_by_world",
                    Motif::ReferenceInconnue {
                        genre: "réglage",
                        id: id.clone(),
                    },
                ));
            }
        }
    }
    // Contrôles croisés entre réglages.
    for (i, check) in file.check.iter().enumerate() {
        let at = format!("check[{i}]");
        for id in [&check.condition.setting, &check.then.setting] {
            if fiche.setting(id).is_none() {
                return Err(refuse(
                    f,
                    at,
                    Motif::ReferenceInconnue {
                        genre: "réglage",
                        id: id.clone(),
                    },
                ));
            }
        }
        if check.then.must_equal.is_some() == check.then.max.is_some() {
            return Err(refuse(
                f,
                at,
                Motif::ChampIncoherent {
                    champ: "then".into(),
                    avec: "(exactement un de must_equal, max)".into(),
                },
            ));
        }
        check_text(
            f,
            &at,
            "message de contrôle",
            &check.message,
            MAX_HELP_CHARS,
        )?;
    }
    for (i, migration) in file.migration.iter().enumerate() {
        let at = format!("migration[{i}]");
        if migration.from_version == 0 || migration.from_version >= file.version {
            return Err(refuse(
                f,
                at,
                Motif::BorneInvalide {
                    quoi: "`from_version` d'une migration : au moins 1 et avant la version de la fiche",
                },
            ));
        }
    }
    Ok(())
}

/// I3 : un réglage du canal `args` est un `int`, un `bool`, un `choice` ou un `text` d'alphabet fermé ;
/// jamais un secret ; son nom est un littéral ; ses valeurs fixes sont dans l'alphabet.
fn check_arg_setting(setting: &Setting, at: &str) -> Result<(), FicheRefusee> {
    let s = Fichier::Settings;
    let Some(arg) = &setting.arg else {
        return Err(refuse(
            s,
            at,
            Motif::ReglageIncomplet {
                reglage: setting.id.clone(),
                champ: "arg",
            },
        ));
    };
    if !arg_flag_ok(&arg.flag) {
        return Err(refuse(s, at, Motif::ArgumentNonLitteral));
    }
    let form_ok = match (&setting.spec, arg.form) {
        (ValueSpec::Secret { .. }, _) => return Err(refuse(s, at, Motif::SecretEnArgument)),
        (ValueSpec::Bool { .. }, ArgForm::FlagIfTrue) => true,
        (
            ValueSpec::Int { .. } | ValueSpec::Choice { .. } | ValueSpec::Bool { .. },
            ArgForm::FlagValue,
        ) => true,
        (
            ValueSpec::Text {
                charset: Charset::ArgSafe,
                ..
            },
            ArgForm::FlagValue,
        ) => true,
        (ValueSpec::Text { .. }, _) => {
            return Err(refuse(
                s,
                at,
                Motif::ChampIncoherent {
                    champ: "charset".into(),
                    avec: "pour un argument (alphabet `arg_safe` obligatoire)".into(),
                },
            ));
        }
        _ => false,
    };
    if !form_ok {
        return Err(refuse(
            s,
            at,
            Motif::ChampIncoherent {
                champ: "type".into(),
                avec: "pour un argument (int, bool, choice ou text arg_safe)".into(),
            },
        ));
    }
    let closed = |value: &str| match first_refused_char(Charset::ArgSafe, value) {
        Some(caractere) => Err(refuse(s, at, Motif::ArgumentHorsAlphabet { caractere })),
        None => Ok(()),
    };
    match &setting.spec {
        ValueSpec::Choice { choices } => {
            for choice in choices {
                closed(&choice.value)?;
            }
        }
        ValueSpec::Bool { on, off } if arg.form == ArgForm::FlagValue => {
            closed(on)?;
            closed(off)?;
        }
        _ => {}
    }
    if !matches!(setting.spec, ValueSpec::Int { .. }) {
        for value in [&setting.proposed, &setting.game_default]
            .into_iter()
            .flatten()
        {
            closed(value)?;
        }
        if let Some(imposed) = &setting.imposed
            && let Some(value) = &imposed.value
        {
            closed(value)?;
        }
    }
    Ok(())
}
