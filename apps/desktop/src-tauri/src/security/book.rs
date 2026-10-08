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
    erasure_pending: bool,
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
        let (device, erasure_pending) = self
            .servers
            .get(server)
            .map_or((SecurityDeviceDto::Unknown, false), |entry| {
                (entry.device, entry.erasure_pending)
            });
        self.store(
            server,
            alert,
            attack_mode,
            device,
            erasure_pending,
            key_at_hand,
        )
    }

    /// Une lecture `GET /security` : l'état complet, poste compris.
    pub fn on_read(
        &mut self,
        server: &str,
        response: &SecurityResponse,
        key_at_hand: bool,
    ) -> SecurityEvent {
        let (alert, attack_mode, device) = response_parts(response);
        self.store(
            server,
            alert,
            attack_mode,
            device,
            response.erasure_pending,
            key_at_hand,
        )
    }

    fn store(
        &mut self,
        server: &str,
        alert: AlertDto,
        attack_mode: AttackModeDto,
        device: SecurityDeviceDto,
        erasure_pending: bool,
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
                erasure_pending,
            },
        );
        SecurityEvent {
            server_id: server.to_owned(),
            seq,
            alert,
            attack_mode,
            device,
            key_at_hand,
            erasure_pending,
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
            erasure_pending: entry.erasure_pending,
        })
    }

    /// Les serveurs ayant un état connu, dans un ordre stable.
    pub fn servers(&self) -> Vec<String> {
        let mut names: Vec<String> = self.servers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Une session s'ouvre ou se ferme sur ce serveur (connexion, déconnexion) : l'effacement en attente est
    /// remis à zéro. L'agent ne le dit qu'aux administrateurs : ce que disait la session d'avant (peut-être
    /// celle d'un administrateur) ne doit jamais se lire sous celle d'un autre compte. La première lecture de
    /// la nouvelle session le rétablit, ou non. FIX:01M4DNFDC9KF9FXYJ0H2TC2JKX
    pub fn reset_erasure(&mut self, server: &str) {
        if let Some(entry) = self.servers.get_mut(server) {
            entry.erasure_pending = false;
        }
    }

    /// Le serveur est retiré du carnet : on l'oublie (mémoire bornée par le carnet).
    pub fn forget(&mut self, server: &str) {
        self.servers.remove(server);
    }
}

#[cfg(test)]
mod tests {
    use hearth_proto::api::security::{AlertInfo, AttackModeInfo, SessionDevice};

    use super::*;

    fn response(erasure_pending: bool) -> SecurityResponse {
        SecurityResponse {
            alert: AlertInfo {
                own: false,
                since: None,
                others: None,
            },
            attack_mode: AttackModeInfo::off(),
            device: SessionDevice::Proven,
            admin_reauth: None,
            erasure_pending,
        }
    }

    /// L'effacement en attente (HRT-32 suites) vient de la lecture `GET /security`, que le flux ne porte pas :
    /// un message du flux garde ce que la dernière lecture en a dit, et la lecture suivante fait foi.
    #[test]
    fn the_pending_erasure_comes_from_the_read_and_survives_a_stream_message() {
        let mut book = SecurityBook::default();
        assert!(book.on_read("forge", &response(true), true).erasure_pending);
        let view = SecurityView {
            alert: response(false).alert,
            attack_mode: response(false).attack_mode,
        };
        assert!(
            book.on_stream("forge", &view, true).erasure_pending,
            "le flux ne dit rien de l'effacement : la dernière lecture fait foi"
        );
        assert!(book.current("forge", true).unwrap().erasure_pending);
        assert!(
            !book
                .on_read("forge", &response(false), true)
                .erasure_pending
        );
        assert!(
            !book.on_stream("neuf", &view, true).erasure_pending,
            "jamais lu : rien en attente"
        );
        // Une session s'ouvre ou se ferme : ce que disait la précédente ne se lit pas sous la suivante.
        assert!(book.on_read("forge", &response(true), true).erasure_pending);
        book.reset_erasure("forge");
        assert!(!book.current("forge", true).unwrap().erasure_pending);
        assert!(!book.on_stream("forge", &view, true).erasure_pending);
        book.reset_erasure("inconnu");
    }
}
