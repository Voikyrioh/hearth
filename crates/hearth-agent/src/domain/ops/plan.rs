//! Le plan d'une opération et son journal d'intention (HRT-83, ADR-0045).
//!
//! Le **plan** est figé à l'acceptation : les étapes dans l'ordre, lesquelles sont destructives, et le
//! point de non-retour. Il ne porte JAMAIS un secret (les secrets d'un plan vivent en mémoire, type
//! `Secret`, ou dans la description scellée).
//!
//! Le **journal** est une suite de lignes : chaque changement d'état d'une étape en ajoute une. Pour une
//! étape destructive, la ligne `intent` est validée sur le disque AVANT le geste, la ligne `done` après.
//! C'est ce journal, et lui seul, qui dit à l'agent qui redémarre où en était l'opération.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;

/// Nombre maximal d'étapes d'un plan.
pub const MAX_STEPS: usize = 32;
/// Longueur maximale du nom d'une étape.
pub const MAX_STEP_CHARS: usize = 40;
/// Nombre maximal d'objets créés nommés par une ligne du journal.
pub const MAX_CREATED_PER_ROW: usize = 8;
/// Longueur maximale du chemin relatif d'un objet créé.
pub const MAX_OBJECT_PATH_CHARS: usize = 512;

/// Le nom d'une étape : un code stable (minuscules, chiffres, `_`), jamais un texte libre.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StepName(String);

impl StepName {
    pub fn parse(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && text.chars().count() <= MAX_STEP_CHARS
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        valid.then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for StepName {
    type Error = PlanError;

    fn try_from(value: String) -> Result<Self, PlanError> {
        Self::parse(&value).ok_or(PlanError::StepName)
    }
}

impl From<StepName> for String {
    fn from(value: StepName) -> Self {
        value.0
    }
}

/// Une étape du plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStep {
    pub name: StepName,
    /// Détruit ou remplace quelque chose : écrit son `intent` AVANT le geste.
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanError {
    #[error("nom d'étape invalide")]
    StepName,
    #[error("un plan a entre une et {MAX_STEPS} étapes")]
    StepCount,
    #[error("deux étapes portent le même nom")]
    DuplicateStep,
    #[error("le point de non-retour doit être une étape destructive du plan")]
    NoReturn,
    #[error("plan illisible")]
    Unreadable,
}

/// Le plan figé d'une opération.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpPlan {
    steps: Vec<PlannedStep>,
    /// À partir de cette étape (son `intent` posé), l'annulation répond `TOO_LATE_TO_CANCEL`. Chaque
    /// genre déclare le sien ; `None` = l'opération s'annule jusqu'au bout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    no_return: Option<StepName>,
}

impl OpPlan {
    pub fn new(steps: Vec<PlannedStep>, no_return: Option<StepName>) -> Result<Self, PlanError> {
        if steps.is_empty() || steps.len() > MAX_STEPS {
            return Err(PlanError::StepCount);
        }
        let mut seen = std::collections::HashSet::new();
        if !steps.iter().all(|step| seen.insert(step.name.as_str())) {
            return Err(PlanError::DuplicateStep);
        }
        if let Some(point) = &no_return
            && !steps
                .iter()
                .any(|step| &step.name == point && step.destructive)
        {
            return Err(PlanError::NoReturn);
        }
        Ok(Self { steps, no_return })
    }

    pub fn steps(&self) -> &[PlannedStep] {
        &self.steps
    }

    pub fn no_return(&self) -> Option<&StepName> {
        self.no_return.as_ref()
    }

    pub fn step(&self, name: &StepName) -> Option<&PlannedStep> {
        self.steps.iter().find(|step| &step.name == name)
    }

    pub fn destructive_steps(&self) -> impl Iterator<Item = &PlannedStep> {
        self.steps.iter().filter(|step| step.destructive)
    }

    /// Relit un plan stocké, en re-vérifiant ses règles (une base éditée à la main ne passe pas).
    pub fn from_json(text: &str) -> Result<Self, PlanError> {
        let raw: Self = serde_json::from_str(text).map_err(|_| PlanError::Unreadable)?;
        Self::new(raw.steps, raw.no_return)
    }

    pub fn to_json(&self) -> String {
        // Un plan ne contient que des textes et des booléens : la sérialisation ne peut pas échouer.
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// L'état d'une étape dans le journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepState {
    /// Écrit AVANT le geste.
    Intent,
    /// Écrit APRÈS le geste.
    Done,
    Failed,
    /// Le geste a été défait (retour arrière).
    Undone,
}

impl StepState {
    pub const ALL: [StepState; 4] = [Self::Intent, Self::Done, Self::Failed, Self::Undone];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Undone => "undone",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == text)
    }
}

/// Où vit un objet créé par une opération (liste fermée : une racine que l'agent connaît, jamais un chemin
/// absolu venu de la base).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectRoot {
    /// La racine des sauvegardes.
    Backups,
    /// Le dossier d'attente (hors de tout montage).
    Staging,
    /// Le dossier de données d'un service.
    ServiceData,
}

/// Ce que l'opération a créé pour elle-même.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// Une archive en cours d'écriture : retirée tant que son étape n'est pas `done`.
    PartialArchive,
    /// Une partie reçue d'un envoi : retirée tant que son étape n'est pas `done`.
    ReceivedPart,
    /// Un dossier d'import : retiré tant qu'aucun geste destructif n'a commencé.
    ImportDir,
    /// Le dossier d'attente d'une restauration : retiré tant qu'elle n'a pas commencé.
    StagingDir,
}

/// Un objet créé par une opération, identifié par son emplacement ET son numéro d'inode : avant de le
/// retirer, l'agent revérifie que c'est toujours LUI (un dossier remplacé entre-temps n'est pas touché).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreatedObject {
    pub root: ObjectRoot,
    /// Chemin relatif à la racine, sans `..`, sans `/` initial, sans `\`.
    pub rel: String,
    pub dev: u64,
    pub ino: u64,
    pub kind: ObjectKind,
}

impl CreatedObject {
    /// Le chemin est relatif et reste sous sa racine : pas de composante vide, `.` ou `..`, pas de `\`,
    /// pas de caractère de contrôle, pas de `:` (lecteur Windows), pas d'octet nul.
    pub fn is_well_formed(&self) -> bool {
        !self.rel.is_empty()
            && self.rel.chars().count() <= MAX_OBJECT_PATH_CHARS
            && !self.rel.starts_with('/')
            && !self.rel.contains(['\\', ':'])
            && !self.rel.chars().any(char::is_control)
            && self
                .rel
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
    }
}

/// Ce qu'il faut pour défaire, et ce que l'étape a créé. Jamais de secret. Les champs qui s'ajouteront
/// (ancien nom d'un conteneur, dossier échangé) seront facultatifs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoInfo {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created: Vec<CreatedObject>,
}

impl UndoInfo {
    pub fn is_well_formed(&self) -> bool {
        self.created.len() <= MAX_CREATED_PER_ROW
            && self.created.iter().all(CreatedObject::is_well_formed)
    }

    /// Relit un `undo_json` stocké ; un texte illisible ou qui nomme un chemin hors règle est refusé,
    /// jamais corrigé : l'agent ne retire rien sur la foi d'une ligne douteuse.
    pub fn from_json(text: &str) -> Option<Self> {
        serde_json::from_str::<Self>(text)
            .ok()
            .filter(Self::is_well_formed)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// Une ligne du journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRecord {
    /// Rang dans le journal de l'opération, à partir de 1.
    pub seq: u32,
    pub step: StepName,
    pub state: StepState,
    pub undo: UndoInfo,
    pub at: OffsetDateTime,
}

/// Le dernier état journalisé d'une étape (`None` = aucune ligne).
pub fn latest_state(records: &[StepRecord], step: &StepName) -> Option<StepState> {
    records
        .iter()
        .filter(|record| &record.step == step)
        .max_by_key(|record| record.seq)
        .map(|record| record.state)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn name(text: &str) -> StepName {
        StepName::parse(text).unwrap()
    }

    pub(crate) fn step(text: &str, destructive: bool) -> PlannedStep {
        PlannedStep {
            name: name(text),
            destructive,
        }
    }

    #[test]
    fn a_step_name_is_a_short_lowercase_code() {
        for good in ["pull", "rename_old", "step2", "a"] {
            assert!(StepName::parse(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "Pull",
            "2nd",
            "_x",
            "rename old",
            "é",
            "a-b",
            &"a".repeat(41),
        ] {
            assert!(StepName::parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn a_plan_has_unique_steps_and_a_point_of_no_return_that_is_destructive() {
        let steps = vec![
            step("pull", false),
            step("rename_old", true),
            step("create", true),
        ];
        let plan = OpPlan::new(steps.clone(), Some(name("create"))).unwrap();
        assert_eq!(plan.no_return(), Some(&name("create")));
        assert_eq!(plan.destructive_steps().count(), 2);
        assert_eq!(OpPlan::new(Vec::new(), None), Err(PlanError::StepCount));
        assert_eq!(
            OpPlan::new(vec![step("a", false); 33], None),
            Err(PlanError::StepCount)
        );
        assert_eq!(
            OpPlan::new(vec![step("a", false), step("a", true)], None),
            Err(PlanError::DuplicateStep)
        );
        // Un point de non-retour absent du plan, ou non destructif.
        assert_eq!(
            OpPlan::new(steps.clone(), Some(name("nope"))),
            Err(PlanError::NoReturn)
        );
        assert_eq!(
            OpPlan::new(steps, Some(name("pull"))),
            Err(PlanError::NoReturn)
        );
    }

    #[test]
    fn a_plan_round_trips_and_a_hand_edited_one_is_refused() {
        let plan = OpPlan::new(
            vec![step("pull", false), step("create", true)],
            Some(name("create")),
        )
        .unwrap();
        assert_eq!(OpPlan::from_json(&plan.to_json()).unwrap(), plan);
        assert_eq!(OpPlan::from_json("{"), Err(PlanError::Unreadable));
        assert!(OpPlan::from_json(r#"{"steps":[]}"#).is_err());
        assert!(
            OpPlan::from_json(r#"{"steps":[{"name":"Bad Name","destructive":true}]}"#).is_err()
        );
        assert!(
            OpPlan::from_json(r#"{"steps":[{"name":"a","destructive":false}],"no_return":"a"}"#)
                .is_err()
        );
    }

    #[test]
    fn a_plan_has_no_room_for_a_secret() {
        let json = serde_json::to_value(
            OpPlan::new(vec![step("create", true)], Some(name("create"))).unwrap(),
        )
        .unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["no_return", "steps"]);
    }

    fn object(rel: &str) -> CreatedObject {
        CreatedObject {
            root: ObjectRoot::Backups,
            rel: rel.to_owned(),
            dev: 1,
            ino: 2,
            kind: ObjectKind::PartialArchive,
        }
    }

    #[test]
    fn a_created_object_cannot_name_a_path_outside_its_root() {
        for good in ["a", "dir/archive.tar.gz.part", "01JABC/x"] {
            assert!(object(good).is_well_formed(), "{good}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "a/./b",
            "a/",
            ".",
            "..",
            "a\\b",
            "C:x",
            "a\u{0}b",
            "a\nb",
        ] {
            assert!(!object(bad).is_well_formed(), "{bad:?}");
        }
        assert!(!object(&"a".repeat(MAX_OBJECT_PATH_CHARS + 1)).is_well_formed());
    }

    #[test]
    fn a_doubtful_undo_line_is_refused_whole() {
        let ok = UndoInfo {
            created: vec![object("a/b")],
        };
        assert_eq!(UndoInfo::from_json(&ok.to_json()), Some(ok));
        assert_eq!(UndoInfo::from_json("{}"), Some(UndoInfo::default()));
        assert_eq!(UndoInfo::from_json("pas du json"), None);
        let escaping = r#"{"created":[{"root":"backups","rel":"../x","dev":1,"ino":2,"kind":"partial_archive"}]}"#;
        assert_eq!(UndoInfo::from_json(escaping), None);
        let unknown_root =
            r#"{"created":[{"root":"etc","rel":"x","dev":1,"ino":2,"kind":"partial_archive"}]}"#;
        assert_eq!(UndoInfo::from_json(unknown_root), None);
    }

    #[test]
    fn the_latest_state_of_a_step_is_its_highest_row() {
        let at = OffsetDateTime::UNIX_EPOCH;
        let row = |seq, step: &str, state| StepRecord {
            seq,
            step: name(step),
            state,
            undo: UndoInfo::default(),
            at,
        };
        let records = vec![
            row(1, "a", StepState::Intent),
            row(2, "a", StepState::Done),
            row(3, "b", StepState::Intent),
        ];
        assert_eq!(latest_state(&records, &name("a")), Some(StepState::Done));
        assert_eq!(latest_state(&records, &name("b")), Some(StepState::Intent));
        assert_eq!(latest_state(&records, &name("c")), None);
        // L'ordre du tableau ne compte pas, le rang oui.
        let reversed: Vec<_> = records.into_iter().rev().collect();
        assert_eq!(latest_state(&reversed, &name("a")), Some(StepState::Done));
    }

    #[test]
    fn step_states_parse_back() {
        for state in StepState::ALL {
            assert_eq!(StepState::parse(state.as_str()), Some(state));
        }
        assert_eq!(StepState::parse("maybe"), None);
    }
}
