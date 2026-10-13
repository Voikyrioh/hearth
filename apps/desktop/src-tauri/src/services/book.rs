//! Le dernier état des services de chaque serveur, tenu côté Rust : un numéro de séquence qui croît
//! strictement, rejoué à l'abonnement de l'interface (ADR-0013 point 3) : un état annoncé avant l'écoute de
//! la fenêtre n'est pas perdu. Pur : sans E/S ni Tauri.

use std::collections::HashMap;

use hearth_link::domain::time::WallTime;
use hearth_proto::api::services::ServicesState;

use super::dto::{ServicesEvent, ServicesViewDto};

struct Entry {
    seq: u32,
    engine_received_at: Option<f64>,
    view: ServicesViewDto,
}

fn millis(time: WallTime) -> f64 {
    // Des millisecondes depuis l'époque tiennent dans 53 bits : la conversion est exacte.
    time.as_millis() as f64
}

#[derive(Default)]
pub struct ServicesBook {
    servers: HashMap<String, Entry>,
}

impl ServicesBook {
    /// L'état complet d'un serveur (déjà fusionné par la liaison) : rangé, rendu avec son numéro.
    ///
    /// `engine_received_at` : quand la liaison a reçu la vue du moteur (son étage, pas la fenêtre).
    pub fn on_state(
        &mut self,
        server: &str,
        state: &ServicesState,
        engine_received_at: Option<WallTime>,
    ) -> ServicesEvent {
        self.store(
            server,
            ServicesViewDto::from_state(state),
            engine_received_at.map(millis),
        )
    }

    /// Cet agent ne gère pas les services.
    pub fn on_unsupported(&mut self, server: &str, agent_version: &str) -> ServicesEvent {
        self.store(
            server,
            ServicesViewDto::Unsupported {
                agent_version: agent_version.to_owned(),
            },
            None,
        )
    }

    fn store(
        &mut self,
        server: &str,
        view: ServicesViewDto,
        engine_received_at: Option<f64>,
    ) -> ServicesEvent {
        let seq = self
            .servers
            .get(server)
            .map_or(1, |entry| entry.seq.wrapping_add(1).max(1));
        self.servers.insert(
            server.to_owned(),
            Entry {
                seq,
                engine_received_at,
                view: view.clone(),
            },
        );
        ServicesEvent {
            server_id: server.to_owned(),
            seq,
            engine_received_at,
            view,
        }
    }

    /// Le dernier état connu d'un serveur, sans lecture réseau (rejeu à l'abonnement).
    pub fn current(&self, server: &str) -> Option<ServicesEvent> {
        self.servers.get(server).map(|entry| ServicesEvent {
            server_id: server.to_owned(),
            seq: entry.seq,
            engine_received_at: entry.engine_received_at,
            view: entry.view.clone(),
        })
    }

    /// Le serveur est retiré du carnet : on l'oublie (mémoire bornée par le carnet).
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }
}
