//! Ce que l'adaptateur observe de la machine avant d'installer ou de désinstaller. Le domaine ne
//! lit rien : il décide d'après cette description.

use super::version::Version;

/// Le binaire installé (`/usr/local/bin/hearth-agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryState {
    Absent,
    /// Présent ; `version` est `None` si `--version` n'a rien rendu de lisible.
    Present {
        version: Option<Version>,
        /// Mêmes octets que le binaire qu'on s'apprête à installer.
        identical: bool,
    },
    /// Le système fournit le binaire (installation gérée) : l'agent n'en installe ni n'en retire.
    ProvidedBySystem,
}

/// L'unité de service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitState {
    Absent,
    Present,
    /// Installation gérée : aucune unité écrite par l'agent.
    NotWritten,
}

/// Le dossier de données.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DataState {
    pub dir_exists: bool,
    /// `cert.pem`, `key.pem` et `install_id` sont tous là.
    pub identity: bool,
    /// `hearth.db` existe.
    pub database: bool,
}

impl DataState {
    /// Y a-t-il des données qu'une réinstallation doit conserver ?
    pub fn any(&self) -> bool {
        self.identity || self.database
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub binary: BinaryState,
    pub unit: UnitState,
    /// Le service tourne.
    pub service_active: bool,
    pub data: DataState,
    pub config_exists: bool,
    /// Administrateurs dans la base ; 0 si elle n'existe pas encore.
    pub admin_accounts: u64,
}

impl Observed {
    /// Rien du tout : ni binaire, ni unité, ni donnée, ni configuration.
    pub fn is_blank(&self) -> bool {
        matches!(
            self.binary,
            BinaryState::Absent | BinaryState::ProvidedBySystem
        ) && matches!(self.unit, UnitState::Absent | UnitState::NotWritten)
            && !self.data.any()
            && !self.config_exists
            && !self.service_active
    }
}

#[cfg(test)]
pub(crate) fn blank() -> Observed {
    Observed {
        binary: BinaryState::Absent,
        unit: UnitState::Absent,
        service_active: false,
        data: DataState::default(),
        config_exists: false,
        admin_accounts: 0,
    }
}
