//! Enregistrer des réglages dans le fichier d'un jeu, sans rien perdre : le plan d'un enregistrement et
//! l'analyse d'un texte collé (HRT-108, ADR-0051, BR-FICHE-046 à 048, 050, 051).
//!
//! DOMAINE PUR : le fichier arrive en octets, le plan repart en octets. Aucune E/S, aucune base, aucune
//! horloge. L'écart (le fichier a été réécrit) est jugé AVANT toute retouche : un réglage connu dont la valeur
//! diffère de celle voulue demande un choix, réglage par réglage ; rien n'est écrit sans lui. Un réglage inconnu
//! du fichier n'est jamais touché (le fichier gagne).

use std::collections::BTreeMap;

use super::formats::{ConfigFormat, Refused, Unreadable};
use super::loading::model::Fiche;
use super::settings_state::{
    Applied, Change, Context, Drift, Effect, Reason, Refusal, SettingsState, Slot, apply_changes,
    drift,
};
use crate::domain::secret::Secret;

/// Le choix du compte pour un réglage dont le fichier ne dit plus ce que Hearth croyait (BR-FICHE-048).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftChoice {
    /// « Garder la valeur du fichier » : la description reprend la valeur lue, le fichier n'est pas touché.
    KeepFile,
    /// « Remettre la mienne » : la valeur voulue par Hearth est réécrite dans le fichier.
    KeepMine,
}

/// Pourquoi un enregistrement n'a rien écrit.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveFailure {
    /// Un changement est refusé (type, bornes, figé, imposé, retiré, version, variante, contrôle croisé).
    Refused(Refusal),
    /// Le fichier n'est pas lisible par son format : refusé entier, jamais réécrit (BR-FICHE-046).
    Unreadable(Unreadable),
    /// Le fichier a été réécrit : ces réglages demandent un choix (BR-FICHE-048). Aucune valeur.
    DriftUnresolved(Vec<String>),
    /// Une valeur ne s'écrit pas dans ce format, ou le fichier écrit ne serait plus relisible.
    Write(Refused),
    /// Le canal n'est pas un fichier que Hearth sait retoucher.
    NoFileFormat,
    /// Une clé que la fiche connaît est donnée comme « autre réglage » : elle passe par son réglage.
    KnownKeyAsOther,
}

/// Ce qu'un enregistrement fait : l'état à garder, les octets à écrire (ou rien), les secrets à sceller.
pub struct SavePlan {
    pub state: SettingsState,
    /// `None` : le fichier ne change pas, rien n'est écrit (BR-FICHE-048).
    pub file: Option<Vec<u8>>,
    pub effect: Effect,
    /// Identifiants dont la valeur voulue a changé, et ceux dont on a repris la valeur du fichier.
    pub changed: Vec<String>,
    pub adopted: Vec<String>,
    /// Valeurs de secrets saisis, à sceller (`setting:<id>`) : jamais journalisées.
    pub secrets: Vec<(String, Secret)>,
}

/// Les entrées de clé et de valeur d'un fichier, dans l'ordre du fichier.
pub fn entries_of(
    format: &dyn ConfigFormat,
    bytes: &[u8],
) -> Result<Vec<(String, String)>, Unreadable> {
    let doc = format.parse(bytes)?;
    Ok(doc
        .entries()
        .into_iter()
        .map(|e| (e.key.to_owned(), e.value.to_owned()))
        .collect())
}

/// Retouche un fichier : seules les valeurs demandées changent (`Some` = poser, `None` = retirer la ligne).
/// Un fichier absent est créé quand tout est à poser (rien à préserver).
pub fn apply_edits(
    format: &dyn ConfigFormat,
    bytes: Option<&[u8]>,
    edits: &[(String, Option<String>)],
) -> Result<Vec<u8>, SaveFailure> {
    match bytes {
        None => {
            let pairs: Vec<(String, String)> = edits
                .iter()
                .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
                .collect();
            format.create(&pairs).map_err(SaveFailure::Write)
        }
        Some(bytes) => {
            let mut doc = format.parse(bytes).map_err(SaveFailure::Unreadable)?;
            for (key, value) in edits {
                match value {
                    Some(v) => format.set(&mut doc, key, v).map_err(SaveFailure::Write)?,
                    None => format.unset(&mut doc, key),
                }
            }
            Ok(doc.render())
        }
    }
}

/// Le plan d'un enregistrement de réglages d'un canal fichier.
///
/// Ordre : le fichier est lu (illisible : refus entier) ; l'écart est calculé ; chaque réglage en écart doit avoir
/// reçu un choix, ou être l'objet d'un changement demandé (le changement vaut choix « la mienne ») ; les
/// changements sont jugés ; le fichier est retouché sur ses seuls réglages. Rien n'est écrit si rien n'a
/// réellement changé.
#[allow(clippy::too_many_arguments)]
pub fn plan_save(
    fiche: &Fiche,
    ctx: &Context<'_>,
    state: &SettingsState,
    channel: &str,
    format: &dyn ConfigFormat,
    file: Option<&[u8]>,
    changes: &[(String, Change)],
    choices: &BTreeMap<String, DriftChoice>,
    others: &[(String, Option<String>)],
) -> Result<SavePlan, SaveFailure> {
    let entries = match file {
        Some(bytes) => entries_of(format, bytes).map_err(SaveFailure::Unreadable)?,
        None => Vec::new(),
    };
    // Un fichier absent n'est pas un écart : c'est le quatrième moment d'écriture (BR-FICHE-048), traité à part.
    let (drifts, _unknown): (Vec<Drift>, Vec<String>) = if file.is_some() {
        drift(fiche, state, channel, &entries)
    } else {
        (Vec::new(), Vec::new())
    };
    let asked: Vec<&str> = changes.iter().map(|(id, _)| id.as_str()).collect();
    let mut unresolved: Vec<String> = Vec::new();
    let mut working = state.clone();
    let mut adopted: Vec<String> = Vec::new();
    let mut mine: Vec<(String, Option<String>)> = Vec::new();
    for found in &drifts {
        // Un réglage imposé que le fichier a défait n'est pas un écart à trancher : le repère `imposed_broken`
        // le dit, et aucune écriture ne le corrige (BR-FICHE-048).
        if fiche
            .setting(&found.id)
            .is_none_or(|setting| setting.imposed.is_some())
        {
            continue;
        }
        if asked.contains(&found.id.as_str()) {
            // Le compte change ce réglage : sa valeur gagne, le fichier sera retouché par le changement.
            continue;
        }
        match choices.get(&found.id) {
            Some(DriftChoice::KeepFile) => {
                working = working_with(&working, &found.id, found.file.clone());
                adopted.push(found.id.clone());
            }
            Some(DriftChoice::KeepMine) => {
                if let Some(setting) = fiche.setting(&found.id) {
                    mine.push((setting.key.clone(), found.hearth.clone()));
                }
            }
            None => unresolved.push(found.id.clone()),
        }
    }
    if !unresolved.is_empty() {
        return Err(SaveFailure::DriftUnresolved(unresolved));
    }
    let Applied {
        state: next,
        effect,
        edits,
        secrets,
        changed,
    } = apply_changes(fiche, ctx, &working, changes).map_err(SaveFailure::Refused)?;

    let mut all: Vec<(String, Option<String>)> = mine;
    for edit in &edits {
        if edit.channel == channel {
            all.push((
                edit.key.clone(),
                edit.value.as_ref().map(|v| v.expose().to_owned()),
            ));
        }
    }
    // Les réglages inconnus de la fiche : changer une valeur ou retirer la ligne (BR-FICHE-047). Une clé que la
    // fiche connaît passe par son réglage, jamais par ici.
    for (key, value) in others {
        let known = fiche
            .settings
            .iter()
            .any(|s| s.channel == channel && s.key == *key)
            || fiche
                .retired
                .iter()
                .any(|r| r.channel == channel && r.key == *key);
        if known {
            return Err(SaveFailure::KnownKeyAsOther);
        }
        all.push((key.clone(), value.clone()));
    }
    // Rien à réécrire : le fichier garde ses octets (BR-FICHE-048). Une ligne déjà identique n'est pas retouchée.
    let observed = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    all.retain(|(key, value)| observed(key) != value.as_deref());
    // Un fichier absent n'est créé que s'il y a quelque chose à y poser.
    let bytes = if all.is_empty() {
        None
    } else {
        Some(apply_edits(format, file, &all)?)
    };
    Ok(SavePlan {
        state: next,
        file: bytes,
        effect: if effect == Effect::None && !all.is_empty() {
            Effect::Restart
        } else {
            effect
        },
        changed,
        adopted,
        secrets,
    })
}

impl std::fmt::Debug for SavePlan {
    /// Les octets du fichier peuvent porter un secret (le mot de passe du serveur) : seule leur taille se montre.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavePlan")
            .field("state", &self.state)
            .field("file_bytes", &self.file.as_ref().map(Vec::len))
            .field("effect", &self.effect)
            .field("changed", &self.changed)
            .field("adopted", &self.adopted)
            .field("secrets", &self.secrets.len())
            .finish()
    }
}

fn working_with(state: &SettingsState, id: &str, file: Option<String>) -> SettingsState {
    let mut next = state.clone();
    next.set_slot(id, file.map_or(Slot::Unwritten, Slot::Value));
    next
}

impl From<Refused> for SaveFailure {
    fn from(refused: Refused) -> Self {
        Self::Write(refused)
    }
}

/// Une valeur lue dans le texte collé, jugée contre le réglage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportStatus {
    /// Sera reprise (la valeur diffère de celle voulue et est valide).
    Take,
    /// Déjà la valeur voulue.
    Same,
    /// Refusée : la raison, sans valeur.
    Rejected(Reason),
    /// Réglage imposé par la fiche : jamais modifié par un import.
    Imposed,
    /// Réglage figé après l'installation.
    Frozen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLine {
    pub id: String,
    pub status: ImportStatus,
    /// La valeur collée ; jamais pour un secret.
    pub pasted: Option<String>,
    /// Un secret : sa valeur n'est jamais rendue par l'analyse.
    pub secret: bool,
}

/// Ce que dit l'analyse d'un texte collé (BR-FICHE-052) : rien n'est écrit, rien n'est renvoyé de ce qui est secret.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportAnalysis {
    pub lines: Vec<ImportLine>,
    /// Lignes du texte que la fiche ne connaît pas : ni reprises, ni ajoutées au fichier.
    pub unknown: u32,
    /// Réglages imposés que le texte retire ou change : avertissement (BR-FICHE-046), le fichier garde les siens.
    pub imposed_undone: Vec<String>,
    /// Clés présentes plus d'une fois : la dernière est lue.
    pub duplicates: u32,
}

/// Lit un texte collé avec le format du canal et le juge réglage par réglage. Illisible : refusé entier.
pub fn analyze_import(
    fiche: &Fiche,
    ctx: &Context<'_>,
    state: &SettingsState,
    channel: &str,
    format: &dyn ConfigFormat,
    text: &[u8],
    imposed_values: &BTreeMap<String, String>,
) -> Result<ImportAnalysis, Unreadable> {
    let doc = format.parse(text)?;
    let entries: Vec<(String, String)> = doc
        .entries()
        .into_iter()
        .map(|e| (e.key.to_owned(), e.value.to_owned()))
        .collect();
    let mut out = ImportAnalysis {
        duplicates: u32::try_from(doc.duplicate_keys()).unwrap_or(u32::MAX),
        ..ImportAnalysis::default()
    };
    let value_of = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let mut known_keys: Vec<&str> = Vec::new();
    for setting in fiche.settings.iter().filter(|s| s.channel == channel) {
        known_keys.push(&setting.key);
        if let Some(imposed) = imposed_values.get(&setting.id) {
            if value_of(&setting.key).as_deref() != Some(imposed.as_str()) {
                out.imposed_undone.push(setting.id.clone());
            }
            out.lines.push(ImportLine {
                id: setting.id.clone(),
                status: ImportStatus::Imposed,
                pasted: (!setting.is_secret())
                    .then(|| value_of(&setting.key))
                    .flatten(),
                secret: setting.is_secret(),
            });
            continue;
        }
        let Some(value) = value_of(&setting.key) else {
            continue;
        };
        let secret = setting.is_secret();
        let status = match apply_changes(
            fiche,
            ctx,
            state,
            &[(setting.id.clone(), Change::Set(value.clone()))],
        ) {
            Ok(applied) if applied.changed.is_empty() => ImportStatus::Same,
            Ok(_) => ImportStatus::Take,
            Err(Refusal {
                reason: Reason::Frozen,
                ..
            }) => ImportStatus::Frozen,
            Err(Refusal {
                reason: Reason::Imposed,
                ..
            }) => ImportStatus::Imposed,
            Err(refusal) => ImportStatus::Rejected(refusal.reason),
        };
        out.lines.push(ImportLine {
            id: setting.id.clone(),
            status,
            pasted: (!secret).then_some(value),
            secret,
        });
    }
    let unknown: Vec<&str> = {
        let mut seen: Vec<&str> = Vec::new();
        for (key, _) in &entries {
            let known = known_keys.contains(&key.as_str())
                || fiche
                    .retired
                    .iter()
                    .any(|r| r.channel == channel && r.key == *key);
            if !known && !seen.contains(&key.as_str()) {
                seen.push(key);
            }
        }
        seen
    };
    out.unknown = u32::try_from(unknown.len()).unwrap_or(u32::MAX);
    Ok(out)
}

/// La valeur lue dans un texte collé pour un réglage (reprise d'un import). `None` : la clé n'y est pas.
pub fn imported_value(
    fiche: &Fiche,
    channel: &str,
    format: &dyn ConfigFormat,
    text: &[u8],
    id: &str,
) -> Result<Option<String>, Unreadable> {
    let doc = format.parse(text)?;
    Ok(fiche
        .setting(id)
        .filter(|s| s.channel == channel)
        .and_then(|s| doc.get(&s.key).map(str::to_owned)))
}
