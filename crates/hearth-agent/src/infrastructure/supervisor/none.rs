//! Aucun superviseur : un système sans systemd (ou un poste de développement). Aucun conteneur n'a
//! de propriétaire, aucune unité n'est connue ; rien n'est dit, rien n'est lancé, et tout ordre rend
//! « indisponible ».

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;

use crate::application::ports::{ContainerRef, ServiceSupervisor, SupervisorError};
use crate::domain::services::{UnitLookup, UnitName};

pub struct NoSupervisor;

#[async_trait]
impl ServiceSupervisor for NoSupervisor {
    async fn owners(&self, _containers: &[ContainerRef]) -> HashMap<String, UnitName> {
        HashMap::new()
    }

    async fn unit(&self, _unit: &UnitName) -> UnitLookup {
        UnitLookup::Unavailable
    }

    async fn start(&self, _unit: &UnitName, _wait: Duration) -> Result<(), SupervisorError> {
        Err(SupervisorError::Unavailable)
    }

    async fn stop(&self, _unit: &UnitName, _wait: Duration) -> Result<(), SupervisorError> {
        Err(SupervisorError::Unavailable)
    }

    async fn restart(&self, _unit: &UnitName, _wait: Duration) -> Result<(), SupervisorError> {
        Err(SupervisorError::Unavailable)
    }
}
