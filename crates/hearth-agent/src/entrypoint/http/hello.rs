use axum::Json;
use axum::extract::State;
use hearth_proto::api::hello::{ApiRange, HelloResponse};

use super::AppState;
use crate::application::hello::AgentDescription;

/// `GET /api/v1/hello` : identité publique de l'agent, sans authentification.
pub async fn hello(State(state): State<AppState>) -> Json<HelloResponse> {
    Json(to_response(state.hello.describe()))
}

fn to_response(description: &AgentDescription) -> HelloResponse {
    HelloResponse {
        product: description.product.to_owned(),
        agent_version: description.agent_version.to_owned(),
        api: ApiRange {
            min: description.api_min,
            max: description.api_max,
        },
        machine_name: description.machine_name.clone(),
        install_id: description.install_id.to_string(),
        managed: description.managed,
        mac_addresses: description.mac_addresses.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::install_id::InstallId;

    #[test]
    fn converts_the_description_to_the_wire_type() {
        let description = AgentDescription {
            product: "hearth",
            agent_version: "1.2.3",
            api_min: 1,
            api_max: 2,
            machine_name: "forge".into(),
            install_id: InstallId::from_bytes([9; 16]),
            managed: true,
            mac_addresses: vec!["AA:BB:CC:DD:EE:FF".into()],
        };
        let wire = to_response(&description);
        assert_eq!(wire.product, "hearth");
        assert_eq!(wire.agent_version, "1.2.3");
        assert_eq!((wire.api.min, wire.api.max), (1, 2));
        assert_eq!(wire.install_id, "09090909090909090909090909090909");
        assert!(wire.managed);
        assert_eq!(wire.mac_addresses, vec!["AA:BB:CC:DD:EE:FF"]);
    }
}
