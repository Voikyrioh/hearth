//! L'état de sécurité tenu côté Rust, par serveur : le dernier état connu, un numéro de séquence qui
//! croît strictement, et ce que l'agent a dit de la session de ce poste. Pur : sans E/S ni Tauri.
//! Rejoué à l'abonnement de l'interface (ADR-0013 point 3) : un état annoncé avant l'écoute de la
//! fenêtre n'est pas perdu.

use std::collections::HashMap;

use hearth_proto::api::security::{SecurityResponse, SecurityView};

use super::dto::{
    AlertDto, AttackModeDto, SecurityDeviceDto, SecurityEvent, response_parts, view_parts,
};

struct Entry {
    seq: u32,
    alert: AlertDto,
    attack_mode: AttackModeDto,
    device: SecurityDeviceDto,
}

#[derive(Default)]
pub struct SecurityBook {
    servers: HashMap<String, Entry>,
}

impl SecurityBook {
    /// Un message `security` du flux : l'alerte et le mode attaque changent ; ce que l'agent a dit de
    /// la session n'y est pas, on garde la dernière lecture (`Unknown` avant la première).
    pub fn on_stream(
        &mut self,
        server: &str,
        view: &SecurityView,
        key_at_hand: bool,
    ) -> SecurityEvent {
        let (alert, attack_mode) = view_parts(view);
        let device = self
            .servers
            .get(server)
            .map_or(SecurityDeviceDto::Unknown, |entry| entry.device);
        self.store(server, alert, attack_mode, device, key_at_hand)
    }

    /// Une lecture `GET /security` : l'état complet, poste compris.
    pub fn on_read(
        &mut self,
        server: &str,
        response: &SecurityResponse,
        key_at_hand: bool,
    ) -> SecurityEvent {
        let (alert, attack_mode, device) = response_parts(response);
        self.store(server, alert, attack_mode, device, key_at_hand)
    }

    fn store(
        &mut self,
        server: &str,
        alert: AlertDto,
        attack_mode: AttackModeDto,
        device: SecurityDeviceDto,
        key_at_hand: bool,
    ) -> SecurityEvent {
        let seq = self
            .servers
            .get(server)
            .map_or(1, |entry| entry.seq.wrapping_add(1).max(1));
        self.servers.insert(
            server.to_owned(),
            Entry {
                seq,
                alert: alert.clone(),
                attack_mode: attack_mode.clone(),
                device,
            },
        );
        SecurityEvent {
            server_id: server.to_owned(),
            seq,
            alert,
            attack_mode,
            device,
            key_at_hand,
        }
    }

    /// Le dernier état connu d'un serveur, sans lecture réseau (rejeu à l'abonnement).
    pub fn current(&self, server: &str, key_at_hand: bool) -> Option<SecurityEvent> {
        self.servers.get(server).map(|entry| SecurityEvent {
            server_id: server.to_owned(),
            seq: entry.seq,
            alert: entry.alert.clone(),
            attack_mode: entry.attack_mode.clone(),
            device: entry.device,
            key_at_hand,
        })
    }

    /// Les serveurs ayant un état connu, dans un ordre stable.
    pub fn servers(&self) -> Vec<String> {
        let mut names: Vec<String> = self.servers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Le serveur est retiré du carnet : on l'oublie (mémoire bornée par le carnet).
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }
}
