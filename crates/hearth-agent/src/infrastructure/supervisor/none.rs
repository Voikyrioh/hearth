//! Aucun superviseur : un système sans systemd (ou un poste de développement). Aucun conteneur n'a
//! de propriétaire, aucune unité n'est connue ; rien n'est dit, rien n'est lancé.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::application::ports::{ContainerRef, ServiceSupervisor};
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
}
