//! Le chargeur : octets et origine en entrée, fiche acceptée ou refus typé en sortie. Il ne lit aucun
//! fichier : l'appelant (`build.rs`, `xtask`, les tests, un jour un catalogue distant) donne les octets.
//!
//! Ordre : taille, UTF-8, analyse fermée des deux fichiers, bornes (I9), invariants de `fiche.toml`
//! (I1 à I8), résolution des préréglages, cohérence interne des réglages, invariants croisés.

use std::collections::BTreeSet;

use super::error::{FicheRefusee, Fichier, Motif};
use super::invariants;
use super::limits::*;
use super::model::{Fiche, Setting, ValueRefusal, ValueSpec, fiche_digest};
use super::schema::fiche::FicheFile;
use super::schema::settings::{
    ChoiceSet, Imposed, ItemCharset, Level, RawSetting, SettingsFile, Source, ValueType, Versions,
};
use super::value::{Charset, ident_ok, is_risky_char, parse_dotted};

/// D'où viennent les octets. Seule l'origine `Embedded` est acceptée aujourd'hui : un catalogue distant
/// exigera une signature (ADR-0050, point 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Embedded,
    Remote,
}

pub const NAME_ONLY_LABEL: &str = "Réglage du jeu, sans explication";

pub fn load(
    origin: Origin,
    fiche_toml: &[u8],
    settings_toml: &[u8],
) -> Result<Fiche, FicheRefusee> {
    if origin == Origin::Remote {
        return Err(FicheRefusee::new(
            Fichier::Fiche,
            "",
            Motif::OrigineNonSupportee,
        ));
    }
    let total = fiche_toml.len().saturating_add(settings_toml.len());
    if total > MAX_FILES_BYTES {
        return Err(FicheRefusee::new(
            Fichier::Fiche,
            "",
            Motif::BorneDepassee {
                quoi: "taille cumulée des deux fichiers (octets)",
                limite: MAX_FILES_BYTES,
                trouve: total,
            },
        ));
    }
    let fiche_text = text_of(Fichier::Fiche, fiche_toml)?;
    let settings_text = text_of(Fichier::Settings, settings_toml)?;
    let file: FicheFile =
        toml::from_str(fiche_text).map_err(|e| map_toml(Fichier::Fiche, fiche_text, &e))?;
    if file.format != 2 {
        return Err(FicheRefusee::new(
            Fichier::Fiche,
            "format",
            Motif::FormatInattendu {
                trouve: file.format,
            },
        ));
    }
    let sfile: SettingsFile = toml::from_str(settings_text)
        .map_err(|e| map_toml(Fichier::Settings, settings_text, &e))?;

    invariants::check_bounds(&file, &sfile)?;
    invariants::check_fiche_file(&file)?;
    let settings = resolve_settings(&file, &sfile)?;
    check_settings(&file, &sfile, &settings)?;
    let fiche = Fiche {
        digest: fiche_digest(fiche_toml, settings_toml),
        themes: sfile.theme,
        retired: sfile.retired,
        settings,
        file,
    };
    invariants::check_cross(&fiche)?;
    Ok(fiche)
}

fn text_of(fichier: Fichier, bytes: &[u8]) -> Result<&str, FicheRefusee> {
    std::str::from_utf8(bytes).map_err(|_| {
        FicheRefusee::new(
            fichier,
            "",
            Motif::Syntaxe {
                detail: "le fichier n'est pas de l'UTF-8 valide".into(),
            },
        )
    })
}

/// Traduit une erreur de `toml` en refus typé : champ inconnu, variante inconnue, champ manquant, sinon
/// syntaxe. Le message de `toml` est la seule source de la nature de l'erreur.
pub(super) fn map_toml(fichier: Fichier, text: &str, error: &toml::de::Error) -> FicheRefusee {
    let message = error.message();
    let ligne = error
        .span()
        .map(|span| text[..span.start.min(text.len())].matches('\n').count() + 1);
    let quoted = |prefix: &str| {
        message.strip_prefix(prefix).map(|rest| {
            rest.split('`')
                .next()
                .unwrap_or("")
                .chars()
                .take(64)
                .collect::<String>()
        })
    };
    let motif = if let Some(champ) = quoted("unknown field `") {
        Motif::ChampInconnu { champ }
    } else if message.starts_with("unknown variant")
        || message.contains("did not match any variant")
    {
        Motif::VarianteInconnue
    } else if let Some(champ) = quoted("missing field `") {
        Motif::ChampManquant { champ }
    } else {
        let first_line = message.lines().next().unwrap_or("");
        Motif::Syntaxe {
            detail: first_line.chars().take(200).collect(),
        }
    };
    FicheRefusee {
        fichier,
        chemin: String::new(),
        ligne,
        motif,
    }
}

fn refuse(fichier: Fichier, chemin: impl Into<String>, motif: Motif) -> FicheRefusee {
    FicheRefusee::new(fichier, chemin, motif)
}

// ---- Résolution des préréglages -----------------------------------------------------------------

macro_rules! inherit {
    ($setting:expr, $preset:expr, $($field:ident),+ $(,)?) => {
        $( if $setting.$field.is_none() { $setting.$field = $preset.$field.clone(); } )+
    };
}

fn merge_preset(setting: &RawSetting, preset: &RawSetting) -> RawSetting {
    let mut merged = setting.clone();
    inherit!(
        merged,
        preset,
        channel,
        arg,
        theme,
        level,
        value_type,
        charset,
        max_len,
        min_len,
        secret_kind,
        obtain_url,
        min,
        max,
        on,
        off,
        choices,
        choice_set,
        item_charset,
        separator,
        max_items,
        game_default,
        proposed,
        change,
        imposed,
        versions,
        source,
        only_variants,
    );
    merged
}

fn resolve_settings(file: &FicheFile, sfile: &SettingsFile) -> Result<Vec<Setting>, FicheRefusee> {
    let mut preset_ids = BTreeSet::new();
    for preset in &sfile.preset {
        let at = format!("preset[{}]", preset.id);
        if !ident_ok(&preset.id, 1, MAX_ID_CHARS, true) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide { ou: "préréglage" },
            ));
        }
        if !preset_ids.insert(preset.id.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantEnDouble {
                    genre: "préréglage",
                    id: preset.id.clone(),
                },
            ));
        }
        for (name, present) in [
            ("key", preset.key.is_some()),
            ("label", preset.label.is_some()),
            ("help", preset.help.is_some()),
            ("preset", preset.preset.is_some()),
        ] {
            if present {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::ChampIncoherent {
                        champ: name.into(),
                        avec: "dans un préréglage (un seul niveau, sans identité)".into(),
                    },
                ));
            }
        }
    }
    let mut seen_ids = BTreeSet::new();
    let mut out = Vec::with_capacity(sfile.setting.len());
    for raw in &sfile.setting {
        let at = format!("setting[{}]", raw.id);
        if !ident_ok(&raw.id, 1, MAX_ID_CHARS, true) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide { ou: "réglage" },
            ));
        }
        if !seen_ids.insert(raw.id.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantEnDouble {
                    genre: "réglage",
                    id: raw.id.clone(),
                },
            ));
        }
        let merged = match &raw.preset {
            Some(preset_id) => {
                let Some(preset) = sfile.preset.iter().find(|p| &p.id == preset_id) else {
                    return Err(refuse(
                        Fichier::Settings,
                        at,
                        Motif::ReferenceInconnue {
                            genre: "préréglage",
                            id: preset_id.clone(),
                        },
                    ));
                };
                merge_preset(raw, preset)
            }
            None => raw.clone(),
        };
        out.push(build_setting(file, sfile, &merged, &at)?);
    }
    Ok(out)
}

fn missing(raw: &RawSetting, champ: &'static str) -> FicheRefusee {
    refuse(
        Fichier::Settings,
        format!("setting[{}]", raw.id),
        Motif::ReglageIncomplet {
            reglage: raw.id.clone(),
            champ,
        },
    )
}

fn build_setting(
    _file: &FicheFile,
    sfile: &SettingsFile,
    raw: &RawSetting,
    at: &str,
) -> Result<Setting, FicheRefusee> {
    let key = raw.key.clone().ok_or_else(|| missing(raw, "key"))?;
    let channel = raw.channel.clone().ok_or_else(|| missing(raw, "channel"))?;
    let theme = raw.theme.clone().ok_or_else(|| missing(raw, "theme"))?;
    let level = raw.level.ok_or_else(|| missing(raw, "level"))?;
    let label = raw.label.clone().ok_or_else(|| missing(raw, "label"))?;
    let help = raw.help.clone().ok_or_else(|| missing(raw, "help"))?;
    let value_type = raw.value_type.ok_or_else(|| missing(raw, "type"))?;
    let change = raw.change.ok_or_else(|| missing(raw, "change"))?;
    let source = raw.source.ok_or_else(|| missing(raw, "source"))?;

    // Les champs propres à un type sont refusés avec un autre type.
    let specific: [(&str, bool, &[ValueType]); 14] = [
        ("charset", raw.charset.is_some(), &[ValueType::Text]),
        (
            "max_len",
            raw.max_len.is_some(),
            &[ValueType::Text, ValueType::Secret, ValueType::Opaque],
        ),
        (
            "min_len",
            raw.min_len.is_some(),
            &[ValueType::Text, ValueType::Secret],
        ),
        (
            "secret_kind",
            raw.secret_kind.is_some(),
            &[ValueType::Secret],
        ),
        ("obtain_url", raw.obtain_url.is_some(), &[ValueType::Secret]),
        ("min", raw.min.is_some(), &[ValueType::Int]),
        ("max", raw.max.is_some(), &[ValueType::Int]),
        ("on", raw.on.is_some(), &[ValueType::Bool]),
        ("off", raw.off.is_some(), &[ValueType::Bool]),
        ("choices", raw.choices.is_some(), &[ValueType::Choice]),
        ("choice_set", raw.choice_set.is_some(), &[ValueType::Choice]),
        (
            "item_charset",
            raw.item_charset.is_some(),
            &[ValueType::IdList],
        ),
        ("separator", raw.separator.is_some(), &[ValueType::IdList]),
        ("max_items", raw.max_items.is_some(), &[ValueType::IdList]),
    ];
    for (name, present, allowed) in specific {
        if present && !allowed.contains(&value_type) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ChampIncoherent {
                    champ: name.into(),
                    avec: format!("pour un réglage de type {value_type:?}"),
                },
            ));
        }
    }
    let spec = match value_type {
        ValueType::Text => ValueSpec::Text {
            charset: raw.charset.ok_or_else(|| missing(raw, "charset"))?,
            min_len: raw.min_len.unwrap_or(0),
            max_len: raw.max_len.ok_or_else(|| missing(raw, "max_len"))?,
        },
        ValueType::Secret => ValueSpec::Secret {
            kind: raw.secret_kind.ok_or_else(|| missing(raw, "secret_kind"))?,
            min_len: raw.min_len.unwrap_or(0),
            max_len: raw.max_len.ok_or_else(|| missing(raw, "max_len"))?,
            obtain_url: raw.obtain_url.clone(),
        },
        ValueType::Int => ValueSpec::Int {
            min: raw.min,
            max: raw.max,
        },
        ValueType::Bool => ValueSpec::Bool {
            on: raw.on.clone().ok_or_else(|| missing(raw, "on"))?,
            off: raw.off.clone().ok_or_else(|| missing(raw, "off"))?,
        },
        ValueType::Choice => {
            let choices = match (&raw.choices, &raw.choice_set) {
                (Some(choices), None) => choices.clone(),
                (None, Some(set)) => {
                    let found: Option<&ChoiceSet> = sfile.choice_set.get(set);
                    match found {
                        Some(set) => set.choices.clone(),
                        None => {
                            return Err(refuse(
                                Fichier::Settings,
                                at,
                                Motif::ReferenceInconnue {
                                    genre: "liste de choix",
                                    id: set.clone(),
                                },
                            ));
                        }
                    }
                }
                (Some(_), Some(_)) => {
                    return Err(refuse(
                        Fichier::Settings,
                        at,
                        Motif::ChampIncoherent {
                            champ: "choice_set".into(),
                            avec: "avec `choices` (l'un ou l'autre)".into(),
                        },
                    ));
                }
                (None, None) => return Err(missing(raw, "choices")),
            };
            ValueSpec::Choice { choices }
        }
        ValueType::IdList => {
            let separator = raw
                .separator
                .clone()
                .ok_or_else(|| missing(raw, "separator"))?;
            let mut chars = separator.chars();
            let (Some(sep), None) = (chars.next(), chars.next()) else {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "le séparateur d'une liste est un seul caractère",
                    },
                ));
            };
            if is_risky_char(sep) || sep.is_ascii_alphanumeric() || sep == '-' || sep == '_' {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "séparateur de liste : ni alphanumérique, ni tiret, ni caractère de contrôle",
                    },
                ));
            }
            ValueSpec::IdList {
                item_charset: raw
                    .item_charset
                    .ok_or_else(|| missing(raw, "item_charset"))?,
                separator: sep,
                max_items: raw.max_items.ok_or_else(|| missing(raw, "max_items"))?,
            }
        }
        ValueType::Opaque => ValueSpec::Opaque {
            max_len: raw.max_len.ok_or_else(|| missing(raw, "max_len"))?,
        },
    };
    Ok(Setting {
        id: raw.id.clone(),
        key,
        channel,
        arg: raw.arg.clone(),
        theme,
        level,
        label,
        help,
        spec,
        game_default: raw.game_default.clone(),
        proposed: raw.proposed.clone(),
        change,
        imposed: raw.imposed.clone(),
        versions: raw.versions.clone(),
        source,
        only_variants: raw.only_variants.clone(),
    })
}

// ---- Cohérence interne des réglages -----------------------------------------------------------------

fn check_settings(
    file: &FicheFile,
    sfile: &SettingsFile,
    settings: &[Setting],
) -> Result<(), FicheRefusee> {
    // Thèmes.
    let mut theme_ids = BTreeSet::new();
    for theme in &sfile.theme {
        let at = format!("theme[{}]", theme.id);
        if !ident_ok(&theme.id, 1, MAX_ID_CHARS, true) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide { ou: "thème" },
            ));
        }
        if !theme_ids.insert(theme.id.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantEnDouble {
                    genre: "thème",
                    id: theme.id.clone(),
                },
            ));
        }
        check_text(
            Fichier::Settings,
            &at,
            "libellé de thème",
            &theme.label,
            MAX_LABEL_CHARS,
        )?;
    }
    // Listes de choix partagées.
    for (name, set) in &sfile.choice_set {
        let at = format!("choice_set.{name}");
        check_choices(&at, &set.choices)?;
    }

    let mut essential = 0usize;
    let mut keys: BTreeSet<(&str, &str)> = BTreeSet::new();
    for setting in settings {
        let at = format!("setting[{}]", setting.id);
        let Some(channel) = file.channel.iter().find(|c| c.id == setting.channel) else {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ReferenceInconnue {
                    genre: "canal",
                    id: setting.channel.clone(),
                },
            ));
        };
        if !theme_ids.contains(setting.theme.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ReferenceInconnue {
                    genre: "thème",
                    id: setting.theme.clone(),
                },
            ));
        }
        if !keys.insert((setting.channel.as_str(), setting.key.as_str())) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::CleEnDouble {
                    canal: setting.channel.clone(),
                    cle: setting.key.clone(),
                },
            ));
        }
        if setting.key.is_empty()
            || setting.key.chars().count() > MAX_LITERAL_CHARS
            || setting.key.chars().any(is_risky_char)
        {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide {
                    ou: "clé de réglage",
                },
            ));
        }
        check_text(
            Fichier::Settings,
            &at,
            "libellé",
            &setting.label,
            MAX_LABEL_CHARS,
        )?;
        check_text(
            Fichier::Settings,
            &at,
            "aide",
            &setting.help,
            MAX_HELP_CHARS,
        )?;
        if let Some(only) = &setting.only_variants {
            check_variant_refs(file, Fichier::Settings, &at, only)?;
        }
        if setting.level == Level::Essential {
            essential += 1;
        }
        if setting.level == Level::Essential && setting.imposed.is_some() {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ReglageImposeEssentiel {
                    reglage: setting.id.clone(),
                },
            ));
        }
        if setting.source == Source::NameOnly
            && (setting.label != NAME_ONLY_LABEL || setting.level != Level::Advanced)
        {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ReglageNameOnlyIncoherent {
                    reglage: setting.id.clone(),
                },
            ));
        }
        check_spec_bounds(&at, setting)?;
        if let ValueSpec::Choice { choices } = &setting.spec {
            check_choices(&at, choices)?;
        }
        if let Some(versions) = &setting.versions {
            check_versions(&at, versions)?;
        }
        check_values(file, channel, setting, &at)?;
        if let Some(imposed) = &setting.imposed {
            check_imposed(file, settings, setting, imposed, &at)?;
        }
    }
    if essential > MAX_ESSENTIAL {
        return Err(refuse(
            Fichier::Settings,
            "",
            Motif::TropDEssentiels { trouve: essential },
        ));
    }

    // Réglages retirés.
    let mut all_ids: BTreeSet<&str> = settings.iter().map(|s| s.id.as_str()).collect();
    for retired in &sfile.retired {
        let at = format!("retired[{}]", retired.id);
        if !ident_ok(&retired.id, 1, MAX_ID_CHARS, true) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide {
                    ou: "réglage retiré",
                },
            ));
        }
        if !all_ids.insert(retired.id.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantEnDouble {
                    genre: "réglage",
                    id: retired.id.clone(),
                },
            ));
        }
        if !file.channel.iter().any(|c| c.id == retired.channel) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::ReferenceInconnue {
                    genre: "canal",
                    id: retired.channel.clone(),
                },
            ));
        }
        if parse_dotted(&retired.until).is_none() {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::BorneInvalide {
                    quoi: "`until` d'un réglage retiré : version numérique pointée",
                },
            ));
        }
        check_text(
            Fichier::Settings,
            &at,
            "libellé",
            &retired.label,
            MAX_LABEL_CHARS,
        )?;
        check_text(
            Fichier::Settings,
            &at,
            "raison",
            &retired.why,
            MAX_WHY_CHARS,
        )?;
        if retired.key.is_empty()
            || retired.key.chars().count() > MAX_LITERAL_CHARS
            || retired.key.chars().any(is_risky_char)
        {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantInvalide {
                    ou: "clé de réglage retiré",
                },
            ));
        }
    }
    for retired in &sfile.retired {
        if let Some(by) = &retired.replaced_by
            && !all_ids.contains(by.as_str())
        {
            return Err(refuse(
                Fichier::Settings,
                format!("retired[{}]", retired.id),
                Motif::ReferenceInconnue {
                    genre: "réglage de remplacement",
                    id: by.clone(),
                },
            ));
        }
    }
    Ok(())
}

pub(super) fn check_text(
    fichier: Fichier,
    at: &str,
    quoi: &'static str,
    text: &str,
    max: usize,
) -> Result<(), FicheRefusee> {
    let len = text.chars().count();
    if len == 0 {
        return Err(refuse(
            fichier,
            at,
            Motif::ChampManquant { champ: quoi.into() },
        ));
    }
    if len > max {
        return Err(refuse(
            fichier,
            at,
            Motif::BorneDepassee {
                quoi,
                limite: max,
                trouve: len,
            },
        ));
    }
    if text.chars().any(is_risky_char) {
        return Err(refuse(
            fichier,
            at,
            Motif::Syntaxe {
                detail: format!("caractère de contrôle ou invisible dans {quoi}"),
            },
        ));
    }
    Ok(())
}

pub(super) fn check_variant_refs(
    file: &FicheFile,
    fichier: Fichier,
    at: &str,
    ids: &[String],
) -> Result<(), FicheRefusee> {
    // `only_variants = []` n'est pas « toutes » : l'absence du champ l'est. Une liste vide serait lue de deux façons
    // (par la fiche, par le client), donc elle est refusée (BR-FICHE-060).
    if ids.is_empty() {
        return Err(refuse(fichier, at, Motif::ListeDeVariantesVide));
    }
    for id in ids {
        if !file.variant.iter().any(|v| &v.id == id) {
            return Err(refuse(
                fichier,
                at,
                Motif::ReferenceInconnue {
                    genre: "variante",
                    id: id.clone(),
                },
            ));
        }
    }
    Ok(())
}

fn check_choices(
    at: &str,
    choices: &[super::schema::settings::Choice],
) -> Result<(), FicheRefusee> {
    if choices.is_empty() {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::ChampManquant {
                champ: "choices".into(),
            },
        ));
    }
    if choices.len() > MAX_CHOICES {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::BorneDepassee {
                quoi: "choix d'une liste",
                limite: MAX_CHOICES,
                trouve: choices.len(),
            },
        ));
    }
    let mut seen = BTreeSet::new();
    for choice in choices {
        if choice.value.chars().count() > MAX_LITERAL_CHARS
            || choice.value.chars().any(is_risky_char)
        {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::Syntaxe {
                    detail: "valeur de choix illisible".into(),
                },
            ));
        }
        if !seen.insert(choice.value.as_str()) {
            return Err(refuse(
                Fichier::Settings,
                at,
                Motif::IdentifiantEnDouble {
                    genre: "valeur de choix",
                    id: choice.value.chars().take(32).collect(),
                },
            ));
        }
        check_text(
            Fichier::Settings,
            at,
            "libellé de choix",
            &choice.label,
            MAX_LABEL_CHARS,
        )?;
    }
    Ok(())
}

fn check_versions(at: &str, versions: &Versions) -> Result<(), FicheRefusee> {
    for (name, v) in [("since", &versions.since), ("until", &versions.until)] {
        if let Some(v) = v
            && parse_dotted(v).is_none()
        {
            let quoi = if name == "since" {
                "`since` : version numérique pointée"
            } else {
                "`until` : version numérique pointée"
            };
            return Err(refuse(Fichier::Settings, at, Motif::BorneInvalide { quoi }));
        }
    }
    Ok(())
}

fn check_spec_bounds(at: &str, setting: &Setting) -> Result<(), FicheRefusee> {
    let too_long = |quoi: &'static str, limite: usize, trouve: usize| {
        refuse(
            Fichier::Settings,
            at,
            Motif::BorneDepassee {
                quoi,
                limite,
                trouve,
            },
        )
    };
    match &setting.spec {
        ValueSpec::Text {
            charset,
            min_len,
            max_len,
        } => {
            if *max_len == 0 || min_len > max_len {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "longueurs d'un texte",
                    },
                ));
            }
            if *max_len > MAX_TEXT_VALUE_CHARS {
                return Err(too_long(
                    "longueur maximale d'un texte",
                    MAX_TEXT_VALUE_CHARS,
                    *max_len,
                ));
            }
            if *charset == Charset::ArgSafe && *max_len > MAX_ARG_VALUE_CHARS {
                return Err(too_long(
                    "longueur d'une valeur d'argument",
                    MAX_ARG_VALUE_CHARS,
                    *max_len,
                ));
            }
        }
        ValueSpec::Secret {
            min_len,
            max_len,
            obtain_url,
            ..
        } => {
            if *max_len == 0 || min_len > max_len {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "longueurs d'un secret",
                    },
                ));
            }
            if *max_len > MAX_TEXT_VALUE_CHARS {
                return Err(too_long(
                    "longueur maximale d'un secret",
                    MAX_TEXT_VALUE_CHARS,
                    *max_len,
                ));
            }
            if let Some(url) = obtain_url {
                invariants::check_url(Fichier::Settings, at, url)?;
            }
        }
        ValueSpec::Int { min, max } => {
            if let (Some(min), Some(max)) = (min, max)
                && min > max
            {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "min supérieur à max",
                    },
                ));
            }
        }
        ValueSpec::Bool { on, off } => {
            if on == off
                || on.is_empty()
                || off.is_empty()
                || on.chars().any(is_risky_char)
                || off.chars().any(is_risky_char)
            {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::BorneInvalide {
                        quoi: "valeurs `on` et `off` distinctes et lisibles",
                    },
                ));
            }
        }
        ValueSpec::IdList { max_items, .. } => {
            if *max_items == 0 || *max_items > MAX_ID_LIST_ITEMS {
                return Err(too_long(
                    "éléments d'une liste",
                    MAX_ID_LIST_ITEMS,
                    *max_items,
                ));
            }
        }
        ValueSpec::Opaque { max_len } => {
            if *max_len == 0 || *max_len > MAX_TEXT_VALUE_CHARS {
                return Err(too_long(
                    "longueur d'un texte opaque",
                    MAX_TEXT_VALUE_CHARS,
                    *max_len,
                ));
            }
        }
        ValueSpec::Choice { .. } => {}
    }
    Ok(())
}

/// Valeur proposée et défaut du jeu : valides pour leur type et pour leur fichier ; un secret n'en porte pas.
fn check_values(
    file: &FicheFile,
    channel: &super::schema::fiche::Channel,
    setting: &Setting,
    at: &str,
) -> Result<(), FicheRefusee> {
    use super::schema::fiche::{ChannelKind, Quote};
    use super::value::forbidden_in_file;

    if setting.is_secret()
        && (setting.proposed.is_some()
            || setting
                .game_default
                .as_deref()
                .is_some_and(|d| !d.is_empty()))
    {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::SecretAvecValeur {
                reglage: setting.id.clone(),
            },
        ));
    }
    let double = channel
        .kv
        .as_ref()
        .is_some_and(|kv| kv.quote == Quote::Double);
    for (name, value) in [
        ("proposed", &setting.proposed),
        ("game_default", &setting.game_default),
    ] {
        let Some(value) = value else { continue };
        if setting.is_secret() {
            continue;
        }
        let bad = |raison: &'static str| {
            refuse(
                Fichier::Settings,
                at,
                Motif::ValeurProposeeInvalide {
                    reglage: format!("{} ({name})", setting.id),
                    raison,
                },
            )
        };
        if let Err(refusal) = super::model::validate_value(&setting.spec, value) {
            // Un argument dit le caractère fautif, jamais la valeur (BR-FICHE-007).
            if channel.kind == ChannelKind::Args
                && let ValueRefusal::Charset(Some(caractere)) = refusal
            {
                return Err(refuse(
                    Fichier::Settings,
                    at,
                    Motif::ArgumentHorsAlphabet { caractere },
                ));
            }
            return Err(bad("hors du type du réglage"));
        }
        if channel.kind == ChannelKind::File
            && let Some(format) = channel.format
            && forbidden_in_file(format, double, value).is_some()
        {
            return Err(bad("caractère interdit dans un fichier de ce format"));
        }
    }
    let _ = file;
    Ok(())
}

fn check_imposed(
    file: &FicheFile,
    settings: &[Setting],
    setting: &Setting,
    imposed: &Imposed,
    at: &str,
) -> Result<(), FicheRefusee> {
    let origins = [
        imposed.value.is_some(),
        imposed.derive.is_some(),
        imposed.secret.is_some(),
        imposed.port.is_some(),
    ]
    .iter()
    .filter(|p| **p)
    .count();
    if origins != 1 {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::ChampIncoherent {
                champ: "imposed".into(),
                avec: "(exactement une de value, derive, secret, port)".into(),
            },
        ));
    }
    check_text(
        Fichier::Settings,
        at,
        "raison d'un réglage imposé",
        &imposed.why,
        MAX_WHY_CHARS,
    )?;
    if let Some(secret) = &imposed.secret
        && !file.secret.iter().any(|s| &s.id == secret)
    {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::ReferenceInconnue {
                genre: "secret généré",
                id: secret.clone(),
            },
        ));
    }
    if let Some(port) = &imposed.port
        && !file.port.iter().any(|p| &p.id == port)
    {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::ReferenceInconnue {
                genre: "port",
                id: port.clone(),
            },
        ));
    }
    if let Some(derive) = &imposed.derive {
        invariants::check_derive(Fichier::Settings, at, derive, settings)?;
        invariants::check_derive_target(Fichier::Settings, at, derive, setting)?;
    }
    if let Some(value) = &imposed.value
        && super::model::validate_value(&setting.spec, value).is_err()
    {
        return Err(refuse(
            Fichier::Settings,
            at,
            Motif::ValeurProposeeInvalide {
                reglage: format!("{} (imposed)", setting.id),
                raison: "hors du type du réglage",
            },
        ));
    }
    Ok(())
}

#[allow(dead_code)]
fn _item_charset_is_closed(item: ItemCharset) -> bool {
    matches!(item, ItemCharset::Digits | ItemCharset::Slug)
}
