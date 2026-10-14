//! Cas d'usage « se présenter » : alimente `GET /api/v1/hello`.
//!
//! Il rend une structure applicative ; la conversion vers le type du fil
//! (`hearth_proto::api::hello::HelloResponse`) est faite par `entrypoint/http`.

use hearth_proto::api::service_logs::CAPABILITY_SERVICE_LOGS;
use hearth_proto::api::service_ops::CAPABILITY_SERVICE_OPS;
use hearth_proto::api::services::{
    CAPABILITY_SERVICE_ADOPTION, CAPABILITY_SERVICE_RIGHTS, CAPABILITY_SERVICES,
};
use hearth_proto::product::PRODUCT_NAME;
use hearth_proto::version::{API_MIN_SUPPORTED, API_VERSION};

use crate::application::ports::MachineInfo;
use crate::domain::install_id::InstallId;

/// Identité publique de l'agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDescription {
    pub product: &'static str,
    pub agent_version: &'static str,
    pub api_min: u32,
    pub api_max: u32,
    pub machine_name: String,
    pub install_id: InstallId,
    pub managed: bool,
    pub mac_addresses: Vec<String>,
    /// Ce que cet agent sait faire de plus que le socle (ADR-0038) : le client n'utilise un sujet du
    /// flux ou une route nouvelle que si la capacité est annoncée.
    pub capabilities: Vec<&'static str>,
}

pub struct HelloService {
    description: AgentDescription,
}

impl HelloService {
    /// Le nom de machine et les adresses MAC sont lus une seule fois, au démarrage.
    pub fn new(install_id: InstallId, managed: bool, machine: &dyn MachineInfo) -> Self {
        Self {
            description: AgentDescription {
                product: PRODUCT_NAME,
                agent_version: crate::build_info::VERSION,
                api_min: API_MIN_SUPPORTED,
                api_max: API_VERSION,
                machine_name: machine.machine_name(),
                install_id,
                managed,
                mac_addresses: machine.mac_addresses(),
                capabilities: vec![
                    CAPABILITY_SERVICES,
                    CAPABILITY_SERVICE_RIGHTS,
                    CAPABILITY_SERVICE_LOGS,
                    CAPABILITY_SERVICE_ADOPTION,
                    // Lire, décider et annuler une opération, et la suivre en direct (HRT-144) : ces routes et
                    // ce message existent. Rien d'autre n'est annoncé (ni démarrer, ni déployer).
                    CAPABILITY_SERVICE_OPS,
                ],
            },
        }
    }

    pub fn describe(&self) -> &AgentDescription {
        &self.description
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[derive(Default)]
    struct FakeMachine {
        reads: Cell<u32>,
    }

    impl MachineInfo for FakeMachine {
        fn machine_name(&self) -> String {
            self.reads.set(self.reads.get() + 1);
            "forge".into()
        }
        fn mac_addresses(&self) -> Vec<String> {
            self.reads.set(self.reads.get() + 1);
            vec!["AA:BB:CC:DD:EE:FF".into()]
        }
    }

    #[test]
    fn describes_the_agent_from_its_parts() {
        let id = InstallId::from_bytes([1; 16]);
        let machine = FakeMachine::default();
        let service = HelloService::new(id.clone(), true, &machine);
        let hello = service.describe();
        assert_eq!(hello.product, "hearth");
        assert_eq!(hello.machine_name, "forge");
        assert_eq!(hello.install_id, id);
        assert!(hello.managed);
        assert_eq!(hello.mac_addresses, vec!["AA:BB:CC:DD:EE:FF"]);
        assert_eq!(hello.api_min, API_MIN_SUPPORTED);
        assert_eq!(hello.api_max, API_VERSION);
        assert_eq!(
            hello.capabilities,
            vec![
                "services",
                "service-rights",
                "service-logs",
                "service-adoption",
                "service-ops"
            ]
        );
    }

    #[test]
    fn the_machine_is_read_once_at_construction() {
        let machine = FakeMachine::default();
        let service = HelloService::new(InstallId::from_bytes([1; 16]), false, &machine);
        service.describe();
        service.describe();
        // Un appel pour le nom, un pour les adresses, et aucun de plus ensuite.
        assert_eq!(machine.reads.get(), 2);
    }
}
