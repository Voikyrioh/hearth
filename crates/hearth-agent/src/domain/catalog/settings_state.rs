//! État des réglages d'un service de jeu : trois états par réglage (une valeur, la valeur proposée, ou
//! « non écrit »), validation d'un changement, effet DÉDUIT du canal, écart avec le fichier du jeu,
//! réglages imposés défaits (conception technique 5.3, 11.2 ; BR-FICHE-037 à 049).
//!
//! « Non écrit » est une valeur : le jeu prend son défaut, Hearth n'écrit pas la ligne. Une valeur par défaut
//! non vérifiée n'est jamais inventée. Aucune E/S : le fichier est donné sous forme d'entrées lues.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use crate::domain::secret::Secret;

use super::loading::model::{Fiche, Setting, ValueRefusal, ValueSpec, validate_value};
use super::loading::schema::fiche::{ChannelKind, PortMapping, Quote};
use super::loading::schema::settings::{ChangeWhen, Level};
use super::loading::value::{compare_versions, forbidden_in_file};

/// Ce que l'état retient d'un réglage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// Une valeur voulue (jamais celle d'un secret).
    Value(String),
    /// Un secret saisi ou imposé : sa valeur est scellée ailleurs, jamais dans l'état.
    Secret,
    /// Non écrit : le jeu prend son défaut.
    Unwritten,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsState {
    slots: BTreeMap<String, Slot>,
}

impl SettingsState {
    pub fn slot(&self, id: &str) -> Option<&Slot> {
        self.slots.get(id)
    }

    /// Valeur non secrète d'un réglage, s'il en a une.
    pub fn value(&self, id: &str) -> Option<&str> {
        match self.slots.get(id) {
            Some(Slot::Value(v)) => Some(v),
            _ => None,
        }
    }

    /// Un état relu de la description du service (HRT-108).
    pub fn from_slots(slots: BTreeMap<String, Slot>) -> Self {
        Self { slots }
    }

    pub fn set_slot(&mut self, id: &str, slot: Slot) {
        self.slots.insert(id.to_owned(), slot);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Slot)> {
        self.slots.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// Où en est le service : `Install` accepte tout ; `Installed` fige les réglages `world_creation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Install,
    Installed,
}

/// Pourquoi un changement est refusé (`SETTING_REFUSED`, conception technique 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Bounds,
    Choice,
    Charset,
    Length,
    Frozen,
    Imposed,
    Retired,
    UnknownId,
    Variant,
    /// Hors des versions du jeu où le réglage existe.
    Version,
    /// Un `[[check]]` entre réglages n'est pas respecté (rang du contrôle).
    Check(usize),
    /// Le monde repris porte ce réglage.
    CarriedByWorld,
}

/// Un changement refusé. Cite au plus UN caractère, jamais la valeur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub id: String,
    pub reason: Reason,
    pub character: Option<char>,
}

impl Refusal {
    fn new(id: &str, reason: Reason) -> Self {
        Self {
            id: id.to_owned(),
            reason,
            character: None,
        }
    }
}

/// Ce qu'un changement demande au service. Déduit du canal, la fiche ne le déclare pas (elle ne peut donc
/// pas mentir).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    None,
    /// Canal `file` : le jeu relit son fichier au démarrage.
    Restart,
    /// Canaux `env` et `args` : il faut recréer le service.
    Recreate,
}

pub fn effect_of(fiche: &Fiche, setting: &Setting) -> Effect {
    match fiche.channel(&setting.channel).map(|c| c.kind) {
        Some(ChannelKind::File) => Effect::Restart,
        Some(ChannelKind::Env | ChannelKind::Args) => Effect::Recreate,
        _ => Effect::None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Set(String),
    /// « Non écrit » : retire la ligne, le jeu reprend son défaut.
    Unset,
    /// Remet la valeur proposée ; quand la fiche ne propose rien, retire la ligne.
    Reset,
}

pub struct Context<'a> {
    pub phase: Phase,
    pub variant: Option<&'a str>,
    pub game_version: Option<&'a str>,
    /// Le monde repris porte ses réglages figés : ils ne sont ni posés ni écrits.
    pub world_imported: bool,
}

/// Retouche à faire dans le fichier d'un canal. La valeur d'un secret ne se Debug jamais.
#[derive(Debug)]
pub struct FileEdit {
    pub channel: String,
    pub key: String,
    /// `None` : retirer la ligne.
    pub value: Option<Secret>,
}

#[derive(Debug)]
pub struct Applied {
    pub state: SettingsState,
    pub effect: Effect,
    pub edits: Vec<FileEdit>,
    /// Valeurs de secrets saisis, à sceller (`setting:<id>`).
    pub secrets: Vec<(String, Secret)>,
    /// Identifiants dont l'état a réellement changé.
    pub changed: Vec<String>,
}

// ---- Version du jeu -----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionStatus {
    Applies,
    NotYet,
    Gone,
    /// La version installée n'est pas connue : « Hearth ne connaît pas la version installée du jeu ».
    Unknown,
}

pub fn version_status(setting: &Setting, game_version: Option<&str>) -> VersionStatus {
    let Some(versions) = &setting.versions else {
        return VersionStatus::Applies;
    };
    if versions.since.is_none() && versions.until.is_none() {
        return VersionStatus::Applies;
    }
    let Some(current) = game_version else {
        return VersionStatus::Unknown;
    };
    if let Some(since) = &versions.since
        && compare_versions(current, since) == Some(Ordering::Less)
    {
        return VersionStatus::NotYet;
    }
    if let Some(until) = &versions.until
        && compare_versions(current, until).is_some_and(|o| o != Ordering::Less)
    {
        return VersionStatus::Gone;
    }
    VersionStatus::Applies
}

// ---- Validation d'une valeur -----------------------------------------------------------------------

fn double_quoted(fiche: &Fiche, setting: &Setting) -> bool {
    fiche
        .channel(&setting.channel)
        .and_then(|c| c.kv.as_ref())
        .is_some_and(|kv| kv.quote == Quote::Double)
}

/// Valide `text` pour le réglage : type, bornes, jeu de caractères, puis ce que le fichier du canal sait écrire.
pub fn validate_text(fiche: &Fiche, setting: &Setting, text: &str) -> Result<(), Refusal> {
    let secret = setting.is_secret();
    if let Err(refusal) = validate_value(&setting.spec, text) {
        let (reason, character) = match refusal {
            ValueRefusal::Bounds => (Reason::Bounds, None),
            ValueRefusal::Choice => (Reason::Choice, None),
            ValueRefusal::Charset(c) => (Reason::Charset, if secret { None } else { c }),
            ValueRefusal::Length => (Reason::Length, None),
        };
        return Err(Refusal {
            id: setting.id.clone(),
            reason,
            character,
        });
    }
    if let Some(channel) = fiche.channel(&setting.channel)
        && channel.kind == ChannelKind::File
        && let Some(format) = channel.format
        && let Some(bad) = forbidden_in_file(format, double_quoted(fiche, setting), text)
    {
        return Err(Refusal {
            id: setting.id.clone(),
            reason: Reason::Charset,
            character: if secret { None } else { Some(bad) },
        });
    }
    Ok(())
}

// ---- Résolution à l'installation -------------------------------------------------------------------------

/// Valeur résolue d'un réglage pour la traduction : texte ou secret.
#[derive(Debug)]
pub enum ResolvedValue {
    Plain(String),
    Secret(Secret),
}

impl ResolvedValue {
    pub fn expose(&self) -> &str {
        match self {
            Self::Plain(v) => v,
            Self::Secret(s) => s.expose(),
        }
    }
}

/// Un réglage résolu : `value` absent = non écrit.
#[derive(Debug)]
pub struct Resolved {
    pub id: String,
    pub channel: String,
    pub key: String,
    pub value: Option<ResolvedValue>,
    pub imposed: bool,
}

pub struct InstallContext<'a> {
    pub variant: Option<&'a str>,
    pub world_imported: bool,
    /// Port du serveur choisi pour chaque port de la fiche (identifiant du port). Absent : le port par défaut.
    pub host_ports: &'a BTreeMap<String, u16>,
    /// Secrets générés par l'appelant (le domaine ne tire aucun nombre aléatoire), par identifiant de `[[secret]]`.
    pub generated: &'a BTreeMap<String, Secret>,
}

/// Port du serveur retenu pour un port de la fiche.
pub fn chosen_host_port(fiche: &Fiche, ctx: &InstallContext<'_>, port_id: &str) -> Option<u16> {
    let port = fiche.port(port_id)?;
    Some(
        ctx.host_ports
            .get(port_id)
            .copied()
            .or(port.default_host)
            .unwrap_or(port.container),
    )
}

/// Port tel que le jeu le voit : fixe pour `nat`, le numéro choisi pour `same_number`.
pub fn game_port(fiche: &Fiche, ctx: &InstallContext<'_>, port_id: &str) -> Option<u16> {
    let port = fiche.port(port_id)?;
    match port.mapping {
        PortMapping::Nat => Some(port.container),
        PortMapping::SameNumber => chosen_host_port(fiche, ctx, port_id),
    }
}

fn applies_to_variant(setting: &Setting, variant: Option<&str>) -> bool {
    match (&setting.only_variants, variant) {
        (None, _) => true,
        (Some(only), Some(v)) => only.iter().any(|o| o == v),
        (Some(_), None) => false,
    }
}

fn carried(fiche: &Fiche, id: &str) -> bool {
    fiche
        .file
        .worlds
        .as_ref()
        .is_some_and(|w| w.carried_by_world.iter().any(|c| c == id))
}

/// Valeur imposée d'un réglage, résolue.
fn resolve_imposed(
    fiche: &Fiche,
    ctx: &InstallContext<'_>,
    setting: &Setting,
    resolved: &BTreeMap<String, String>,
) -> Option<ResolvedValue> {
    let imposed = setting.imposed.as_ref()?;
    if let Some(value) = &imposed.value {
        return Some(ResolvedValue::Plain(value.clone()));
    }
    if let Some(derive) = &imposed.derive {
        let from = resolved.get(&derive.from)?;
        return Some(ResolvedValue::Plain(format!(
            "{}{}{}",
            derive.prefix.as_deref().unwrap_or(""),
            from,
            derive.suffix.as_deref().unwrap_or("")
        )));
    }
    if let Some(secret) = &imposed.secret {
        let value = ctx.generated.get(secret)?;
        return Some(ResolvedValue::Secret(Secret::new(
            value.expose().to_owned(),
        )));
    }
    if let Some(port) = &imposed.port {
        return game_port(fiche, ctx, port).map(|p| ResolvedValue::Plain(p.to_string()));
    }
    None
}

/// Valeurs de départ d'un service : imposé, sinon réponse de l'utilisateur, sinon valeur proposée, sinon non
/// écrit. Un réglage porté par le monde repris, ou d'une autre variante, n'est pas écrit.
pub fn resolve_install(
    fiche: &Fiche,
    ctx: &InstallContext<'_>,
    answers: &BTreeMap<String, String>,
) -> Result<Vec<Resolved>, Refusal> {
    for id in answers.keys() {
        let Some(setting) = fiche.setting(id) else {
            let reason = if fiche.retired_setting(id).is_some() {
                Reason::Retired
            } else {
                Reason::UnknownId
            };
            return Err(Refusal::new(id, reason));
        };
        if setting.imposed.is_some() {
            return Err(Refusal::new(id, Reason::Imposed));
        }
        if !applies_to_variant(setting, ctx.variant) {
            return Err(Refusal::new(id, Reason::Variant));
        }
        if ctx.world_imported && carried(fiche, id) {
            return Err(Refusal::new(id, Reason::CarriedByWorld));
        }
    }
    // Premier passage : tout ce qui n'est pas imposé (les valeurs dérivées en dépendent).
    let mut plain: BTreeMap<String, String> = BTreeMap::new();
    let mut out: Vec<Resolved> = Vec::new();
    for setting in &fiche.settings {
        if setting.imposed.is_some() {
            continue;
        }
        let value = if !applies_to_variant(setting, ctx.variant)
            || (ctx.world_imported && carried(fiche, &setting.id))
        {
            None
        } else if let Some(answer) = answers.get(&setting.id) {
            validate_text(fiche, setting, answer)?;
            Some(answer.clone())
        } else if setting.is_secret() {
            None
        } else {
            setting.proposed.clone()
        };
        if let Some(v) = &value
            && !setting.is_secret()
        {
            plain.insert(setting.id.clone(), v.clone());
        }
        out.push(Resolved {
            id: setting.id.clone(),
            channel: setting.channel.clone(),
            key: setting.key.clone(),
            value: value.map(|v| {
                if setting.is_secret() {
                    ResolvedValue::Secret(Secret::new(v))
                } else {
                    ResolvedValue::Plain(v)
                }
            }),
            imposed: false,
        });
    }
    // Second passage : les réglages imposés.
    for setting in &fiche.settings {
        if setting.imposed.is_none() || !applies_to_variant(setting, ctx.variant) {
            continue;
        }
        let value = resolve_imposed(fiche, ctx, setting, &plain);
        out.push(Resolved {
            id: setting.id.clone(),
            channel: setting.channel.clone(),
            key: setting.key.clone(),
            value,
            imposed: true,
        });
    }
    // Remet l'ordre de la fiche.
    out.sort_by_key(|r| {
        fiche
            .settings
            .iter()
            .position(|s| s.id == r.id)
            .unwrap_or(usize::MAX)
    });
    Ok(out)
}

/// L'état de départ d'après les valeurs résolues.
pub fn initial_state(fiche: &Fiche, resolved: &[Resolved]) -> SettingsState {
    let mut slots = BTreeMap::new();
    for setting in &fiche.settings {
        let slot = match resolved
            .iter()
            .find(|r| r.id == setting.id)
            .and_then(|r| r.value.as_ref())
        {
            Some(ResolvedValue::Plain(v)) if !setting.is_secret() => Slot::Value(v.clone()),
            Some(_) => Slot::Secret,
            None => Slot::Unwritten,
        };
        slots.insert(setting.id.clone(), slot);
    }
    SettingsState { slots }
}

// ---- Changements -------------------------------------------------------------------------------------

/// Applique des changements à un état : valide chacun (inconnu, retiré, variante, version, imposé, figé, type,
/// bornes), puis les contrôles croisés `[[check]]` sur l'état obtenu. Rien n'est écrit si un seul est refusé.
pub fn apply_changes(
    fiche: &Fiche,
    ctx: &Context<'_>,
    state: &SettingsState,
    changes: &[(String, Change)],
) -> Result<Applied, Refusal> {
    let mut next = state.clone();
    let mut edits: Vec<FileEdit> = Vec::new();
    let mut secrets: Vec<(String, Secret)> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    let mut effect = Effect::None;

    for (id, change) in changes {
        let Some(setting) = fiche.setting(id) else {
            let reason = if fiche.retired_setting(id).is_some() {
                Reason::Retired
            } else {
                Reason::UnknownId
            };
            return Err(Refusal::new(id, reason));
        };
        if !applies_to_variant(setting, ctx.variant) {
            return Err(Refusal::new(id, Reason::Variant));
        }
        if setting.imposed.is_some() {
            return Err(Refusal::new(id, Reason::Imposed));
        }
        if ctx.world_imported && carried(fiche, id) {
            return Err(Refusal::new(id, Reason::CarriedByWorld));
        }
        match version_status(setting, ctx.game_version) {
            VersionStatus::Gone | VersionStatus::NotYet => {
                return Err(Refusal::new(id, Reason::Version));
            }
            VersionStatus::Applies | VersionStatus::Unknown => {}
        }
        if ctx.phase == Phase::Installed && setting.change == ChangeWhen::WorldCreation {
            return Err(Refusal::new(id, Reason::Frozen));
        }
        let (new_slot, new_text): (Slot, Option<String>) = match change {
            Change::Set(text) => {
                validate_text(fiche, setting, text)?;
                if setting.is_secret() {
                    (Slot::Secret, Some(text.clone()))
                } else {
                    (Slot::Value(text.clone()), Some(text.clone()))
                }
            }
            Change::Unset => (Slot::Unwritten, None),
            Change::Reset => match (&setting.proposed, setting.is_secret()) {
                (Some(p), false) => (Slot::Value(p.clone()), Some(p.clone())),
                _ => (Slot::Unwritten, None),
            },
        };
        let before = next.slots.get(id).cloned().unwrap_or(Slot::Unwritten);
        let differs = before != new_slot || (setting.is_secret() && new_text.is_some());
        next.slots.insert(id.clone(), new_slot);
        if !differs {
            continue;
        }
        changed.push(id.clone());
        effect = effect.max(effect_of(fiche, setting));
        if setting.is_secret()
            && let Some(text) = &new_text
        {
            secrets.push((id.clone(), Secret::new(text.clone())));
        }
        if fiche
            .channel(&setting.channel)
            .is_some_and(|c| c.kind == ChannelKind::File)
        {
            edits.push(FileEdit {
                channel: setting.channel.clone(),
                key: setting.key.clone(),
                value: new_text.map(Secret::new),
            });
        }
    }
    check_rules(fiche, &next)?;
    Ok(Applied {
        state: next,
        effect,
        edits,
        secrets,
        changed,
    })
}

/// Les `[[check]]` de la fiche sur un état : si `if` vaut `equals`, `then` doit respecter `must_equal` ou `max`.
pub fn check_rules(fiche: &Fiche, state: &SettingsState) -> Result<(), Refusal> {
    for (index, check) in fiche.file.check.iter().enumerate() {
        if state.value(&check.condition.setting) != Some(check.condition.equals.as_str()) {
            continue;
        }
        let target = state.value(&check.then.setting);
        let violated = if let Some(expected) = &check.then.must_equal {
            target != Some(expected.as_str())
        } else if let Some(max) = check.then.max {
            target
                .and_then(|t| t.parse::<i64>().ok())
                .is_some_and(|n| n > max)
        } else {
            false
        };
        if violated {
            return Err(Refusal::new(&check.then.setting, Reason::Check(index)));
        }
    }
    Ok(())
}

// ---- Écart avec le fichier du jeu --------------------------------------------------------------------

/// Un écart entre la valeur voulue par Hearth et celle du fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub id: String,
    pub channel: String,
    pub hearth: Option<String>,
    pub file: Option<String>,
}

/// Compare l'état aux entrées lues d'un fichier de canal. Les secrets ne sont pas comparés (leur valeur n'est
/// pas dans l'état). Rend aussi les clés du fichier que la fiche ne connaît pas (« le fichier gagne »).
pub fn drift(
    fiche: &Fiche,
    state: &SettingsState,
    channel: &str,
    entries: &[(String, String)],
) -> (Vec<Drift>, Vec<String>) {
    let observed = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let mut drifts = Vec::new();
    for setting in fiche
        .settings
        .iter()
        .filter(|s| s.channel == channel && !s.is_secret())
    {
        let wanted = state.value(&setting.id).map(str::to_owned);
        let file = observed(&setting.key);
        if wanted != file && state.slot(&setting.id).is_some() {
            drifts.push(Drift {
                id: setting.id.clone(),
                channel: channel.to_owned(),
                hearth: wanted,
                file,
            });
        }
    }
    let known = |key: &str| {
        fiche
            .settings
            .iter()
            .any(|s| s.channel == channel && s.key == key)
            || fiche
                .retired
                .iter()
                .any(|r| r.channel == channel && r.key == key)
    };
    let mut extras: Vec<String> = Vec::new();
    for (key, _) in entries {
        if !known(key) && !extras.contains(key) {
            extras.push(key.clone());
        }
    }
    (drifts, extras)
}

/// Le nom d'un réglage que la fiche ne connaît pas ressemble-t-il à un secret ? Sa valeur n'est alors jamais montrée
/// (BR-FICHE-047) : le nom contient PASS (donc PASSWORD), SECRET, TOKEN ou KEY, sans tenir compte de la casse.
pub fn looks_secret(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    ["PASS", "SECRET", "TOKEN", "KEY"]
        .iter()
        .any(|word| upper.contains(word))
}

/// Réglages imposés que le fichier a défaits (aucune écriture : le repère `imposed_broken` du service).
pub fn imposed_broken(
    fiche: &Fiche,
    resolved: &[Resolved],
    channel_entries: &BTreeMap<String, Vec<(String, String)>>,
) -> Vec<String> {
    let mut broken = Vec::new();
    for r in resolved.iter().filter(|r| r.imposed) {
        let Some(value) = &r.value else { continue };
        let Some(setting) = fiche.setting(&r.id) else {
            continue;
        };
        if fiche
            .channel(&setting.channel)
            .is_none_or(|c| c.kind != ChannelKind::File)
        {
            continue;
        }
        let Some(entries) = channel_entries.get(&setting.channel) else {
            continue;
        };
        let file = entries
            .iter()
            .rev()
            .find(|(k, _)| *k == setting.key)
            .map(|(_, v)| v.as_str());
        if file != Some(value.expose()) {
            broken.push(r.id.clone());
        }
    }
    broken
}

/// Les huit questions au plus de l'installation (niveau `essential`), dans l'ordre de la fiche.
pub fn essential_questions(fiche: &Fiche) -> Vec<&Setting> {
    fiche
        .settings
        .iter()
        .filter(|s| s.level == Level::Essential)
        .collect()
}

/// Libellé d'une valeur lisible pour l'écran, ou `None` pour un secret.
pub fn display_value<'a>(setting: &'a Setting, value: &'a str) -> Option<&'a str> {
    if setting.is_secret() {
        return None;
    }
    if let ValueSpec::Choice { choices } = &setting.spec {
        return choices
            .iter()
            .find(|c| c.value == value)
            .map(|c| c.label.as_str());
    }
    Some(value)
}
