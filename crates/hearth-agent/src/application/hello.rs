//! Cas d'usage « se présenter » : répond à `GET /api/v1/hello`.

use std::sync::Arc;

use hearth_proto::api::hello::{ApiRange, HelloResponse};
use hearth_proto::product::PRODUCT_NAME;
use hearth_proto::version::{API_MIN_SUPPORTED, API_VERSION};

use crate::application::ports::MachineInfo;
use crate::domain::install_id::InstallId;

pub struct HelloService {
    install_id: InstallId,
    managed: bool,
    machine: Arc<dyn MachineInfo>,
}

impl HelloService {
    pub fn new(install_id: InstallId, managed: bool, machine: Arc<dyn MachineInfo>) -> Self {
        Self {
            install_id,
            managed,
            machine,
        }
    }

    pub fn describe(&self) -> HelloResponse {
        HelloResponse {
            product: PRODUCT_NAME.to_owned(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
            api: ApiRange {
                min: API_MIN_SUPPORTED,
                max: API_VERSION,
            },
            machine_name: self.machine.machine_name(),
            install_id: self.install_id.to_string(),
            managed: self.managed,
            mac_addresses: self.machine.mac_addresses(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeMachine;

    impl MachineInfo for FakeMachine {
        fn machine_name(&self) -> String {
            "forge".into()
        }
        fn mac_addresses(&self) -> Vec<String> {
            vec!["AA:BB:CC:DD:EE:FF".into()]
        }
    }

    #[test]
    fn describes_the_agent_from_its_parts() {
        let id = InstallId::from_bytes([1; 16]);
        let service = HelloService::new(id.clone(), true, Arc::new(FakeMachine));
        let hello = service.describe();
        assert_eq!(hello.product, "hearth");
        assert_eq!(hello.machine_name, "forge");
        assert_eq!(hello.install_id, id.as_str());
        assert!(hello.managed);
        assert_eq!(hello.mac_addresses, vec!["AA:BB:CC:DD:EE:FF"]);
        assert_eq!(hello.api.min, API_MIN_SUPPORTED);
        assert_eq!(hello.api.max, API_VERSION);
    }
}
