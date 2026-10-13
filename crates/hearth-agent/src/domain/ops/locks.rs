//! Les verrous (HRT-83, ADR-0045, BR-LIFE-013, BR-DEL-019, BR-FILES-024) : une table PURE, tenue en
//! mémoire par l'agent, sans horloge ni E/S. L'enveloppe qui la protège d'un `Mutex` et rend le verrou à
//! la fin d'une opération est dans `application::ops_locks`.
//!
//! | Mode | Pris par | Compatible avec |
//! |---|---|---|
//! | Exclusif | toute opération (démarrer, déployer, modifier, supprimer, sauvegarder, restaurer…) | rien |
//! | Partagé en écriture | chaque écriture de fichier | d'autres écritures ; REFUSÉ si un exclusif est tenu |
//! | Aucun | les lectures | tout, sauf la lecture de fichiers pendant une restauration (`files_frozen`) |
//!
//! Deux verrous du serveur : `heavy_io` (une place : sauvegarde, restauration, reprise d'un monde) et
//! `names` (un nom en cours de suppression ou de déploiement est réservé).
//!
//! Le verrou est perdu au redémarrage de l'agent, SAUF pour une opération `needs_attention` : la table est
//! alors reconstruite avec [`LockTable::restore_held`].

use std::collections::HashMap;

use hearth_proto::error::ErrorCode;

use super::kind::OpKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Exclusive,
    SharedWrite,
    None,
}

/// Qui tient un verrou de service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Operation(OpKind),
    FileWrites,
}

/// Ce qu'une lecture veut lire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadKind {
    /// Liste, détail, journaux : toujours permis.
    Plain,
    /// Parcourir ou lire des fichiers : refusé pendant une restauration (`files_frozen`).
    Files,
}

/// Pourquoi un verrou est refusé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Le service est pris.
    ServiceBusy { holder: Holder },
    /// Le nom est réservé par une opération (suppression ou déploiement).
    NameReserved { by: OpKind },
    /// Une autre sauvegarde, restauration ou reprise de monde tient la place.
    HeavyIoBusy { service: String },
    /// Les fichiers sont gelés par une restauration.
    FilesFrozen,
}

impl Refusal {
    /// Le code de la réponse (409 dans tous les cas).
    pub const fn error_code(&self) -> ErrorCode {
        match self {
            Self::NameReserved { by: OpKind::Delete } => ErrorCode::NameBeingDeleted,
            Self::HeavyIoBusy { .. } => ErrorCode::HeavyIoBusy,
            Self::ServiceBusy { .. } | Self::NameReserved { .. } | Self::FilesFrozen => {
                ErrorCode::OperationInProgress
            }
        }
    }
}

#[derive(Debug, Default)]
struct ServiceLock {
    /// L'opération qui tient le verrou exclusif.
    exclusive: Option<(String, OpKind)>,
    shared_writers: u32,
    files_frozen: bool,
}

impl ServiceLock {
    fn is_free(&self) -> bool {
        self.exclusive.is_none() && self.shared_writers == 0 && !self.files_frozen
    }
}

#[derive(Debug, Default)]
pub struct LockTable {
    services: HashMap<String, ServiceLock>,
    names: HashMap<String, (String, OpKind)>,
    heavy_io: Option<(String, String)>,
}

impl LockTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prend le verrou exclusif de `service` pour l'opération `op`.
    pub fn acquire_exclusive(
        &mut self,
        service: &str,
        op: &str,
        kind: OpKind,
    ) -> Result<(), Refusal> {
        let lock = self.services.entry(service.to_owned()).or_default();
        if let Some((_, holder)) = &lock.exclusive {
            return Err(Refusal::ServiceBusy {
                holder: Holder::Operation(*holder),
            });
        }
        if lock.shared_writers > 0 {
            return Err(Refusal::ServiceBusy {
                holder: Holder::FileWrites,
            });
        }
        lock.exclusive = Some((op.to_owned(), kind));
        Ok(())
    }

    /// Rend le verrou exclusif ; rien n'est rendu si ce n'est pas `op` qui le tient.
    pub fn release_exclusive(&mut self, service: &str, op: &str) -> bool {
        let Some(lock) = self.services.get_mut(service) else {
            return false;
        };
        if lock.exclusive.as_ref().map(|(held, _)| held.as_str()) != Some(op) {
            return false;
        }
        lock.exclusive = None;
        lock.files_frozen = false;
        if lock.is_free() {
            self.services.remove(service);
        }
        true
    }

    /// Prend une place d'écriture de fichier : permis avec d'autres écritures, refusé sous un exclusif.
    pub fn acquire_shared_write(&mut self, service: &str) -> Result<(), Refusal> {
        let lock = self.services.entry(service.to_owned()).or_default();
        if let Some((_, holder)) = &lock.exclusive {
            return Err(Refusal::ServiceBusy {
                holder: Holder::Operation(*holder),
            });
        }
        lock.shared_writers = lock.shared_writers.saturating_add(1);
        Ok(())
    }

    pub fn release_shared_write(&mut self, service: &str) {
        let Some(lock) = self.services.get_mut(service) else {
            return;
        };
        lock.shared_writers = lock.shared_writers.saturating_sub(1);
        if lock.is_free() {
            self.services.remove(service);
        }
    }

    /// Une lecture est-elle permise ? Les lectures ne prennent aucun verrou.
    pub fn check_read(&self, service: &str, what: ReadKind) -> Result<(), Refusal> {
        let frozen = self
            .services
            .get(service)
            .is_some_and(|lock| lock.files_frozen);
        match what {
            ReadKind::Files if frozen => Err(Refusal::FilesFrozen),
            ReadKind::Files | ReadKind::Plain => Ok(()),
        }
    }

    /// La restauration gèle (ou dégèle) la lecture des fichiers du service qu'elle tient.
    pub fn set_files_frozen(&mut self, service: &str, op: &str, frozen: bool) -> bool {
        match self.services.get_mut(service) {
            Some(lock) if lock.exclusive.as_ref().map(|(held, _)| held.as_str()) == Some(op) => {
                lock.files_frozen = frozen;
                true
            }
            _ => false,
        }
    }

    /// Réserve un nom (déjà replié) pour une opération.
    pub fn reserve_name(&mut self, folded: &str, op: &str, kind: OpKind) -> Result<(), Refusal> {
        if let Some((_, by)) = self.names.get(folded) {
            return Err(Refusal::NameReserved { by: *by });
        }
        self.names.insert(folded.to_owned(), (op.to_owned(), kind));
        Ok(())
    }

    pub fn release_name(&mut self, folded: &str, op: &str) -> bool {
        if self.names.get(folded).is_some_and(|(held, _)| held == op) {
            self.names.remove(folded);
            return true;
        }
        false
    }

    /// Le nom est-il réservé, et par quel genre d'opération ?
    pub fn name_reserved_by(&self, folded: &str) -> Option<OpKind> {
        self.names.get(folded).map(|(_, kind)| *kind)
    }

    /// Prend l'unique place des opérations lourdes en entrées-sorties.
    pub fn acquire_heavy_io(&mut self, service: &str, op: &str) -> Result<(), Refusal> {
        if let Some((held_service, _)) = &self.heavy_io {
            return Err(Refusal::HeavyIoBusy {
                service: held_service.clone(),
            });
        }
        self.heavy_io = Some((service.to_owned(), op.to_owned()));
        Ok(())
    }

    pub fn release_heavy_io(&mut self, op: &str) -> bool {
        if self.heavy_io.as_ref().is_some_and(|(_, held)| held == op) {
            self.heavy_io = None;
            return true;
        }
        false
    }

    /// Rend TOUT ce que tient une opération (fin d'opération, quel que soit le chemin).
    pub fn release_all_of(&mut self, op: &str) {
        let services: Vec<String> = self
            .services
            .iter()
            .filter(|(_, lock)| lock.exclusive.as_ref().is_some_and(|(held, _)| held == op))
            .map(|(service, _)| service.clone())
            .collect();
        for service in services {
            self.release_exclusive(&service, op);
        }
        self.names.retain(|_, (held, _)| held != op);
        self.release_heavy_io(op);
    }

    /// Reconstruit, au démarrage, le verrou d'une opération `needs_attention` : il reste tenu jusqu'à ce
    /// que le compte décide.
    pub fn restore_held(&mut self, service: &str, op: &str, kind: OpKind) -> Result<(), Refusal> {
        self.acquire_exclusive(service, op, kind)
    }

    /// Nombre de services qui ont un verrou (tests, métrologie).
    pub fn held_services(&self) -> usize {
        self.services.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exclusive_lock_refuses_everything_else_on_the_service() {
        let mut table = LockTable::new();
        table
            .acquire_exclusive("h_A", "op1", OpKind::Update)
            .unwrap();
        assert_eq!(
            table.acquire_exclusive("h_A", "op2", OpKind::Stop),
            Err(Refusal::ServiceBusy {
                holder: Holder::Operation(OpKind::Update)
            })
        );
        assert_eq!(
            table.acquire_shared_write("h_A"),
            Err(Refusal::ServiceBusy {
                holder: Holder::Operation(OpKind::Update)
            })
        );
        // Un autre service n'est pas gêné.
        table
            .acquire_exclusive("h_B", "op3", OpKind::Backup)
            .unwrap();
        table.acquire_shared_write("h_C").unwrap();
    }

    #[test]
    fn file_writes_share_the_service_but_keep_an_exclusive_operation_out() {
        let mut table = LockTable::new();
        table.acquire_shared_write("h_A").unwrap();
        table.acquire_shared_write("h_A").unwrap();
        assert_eq!(
            table.acquire_exclusive("h_A", "op1", OpKind::Delete),
            Err(Refusal::ServiceBusy {
                holder: Holder::FileWrites
            })
        );
        table.release_shared_write("h_A");
        assert!(
            table
                .acquire_exclusive("h_A", "op1", OpKind::Delete)
                .is_err()
        );
        table.release_shared_write("h_A");
        table
            .acquire_exclusive("h_A", "op1", OpKind::Delete)
            .unwrap();
    }

    #[test]
    fn only_the_holder_releases_and_a_released_service_leaves_no_trace() {
        let mut table = LockTable::new();
        table
            .acquire_exclusive("h_A", "op1", OpKind::Update)
            .unwrap();
        assert!(!table.release_exclusive("h_A", "intruder"));
        assert!(!table.release_exclusive("h_Z", "op1"));
        assert_eq!(table.held_services(), 1);
        assert!(table.release_exclusive("h_A", "op1"));
        assert_eq!(table.held_services(), 0);
        table.acquire_exclusive("h_A", "op2", OpKind::Stop).unwrap();
    }

    #[test]
    fn reads_take_no_lock_and_only_file_reads_stop_during_a_restore() {
        let mut table = LockTable::new();
        table
            .acquire_exclusive("h_A", "op1", OpKind::Restore)
            .unwrap();
        assert_eq!(table.check_read("h_A", ReadKind::Plain), Ok(()));
        assert_eq!(table.check_read("h_A", ReadKind::Files), Ok(()));
        assert!(table.set_files_frozen("h_A", "op1", true));
        assert_eq!(table.check_read("h_A", ReadKind::Plain), Ok(()));
        assert_eq!(
            table.check_read("h_A", ReadKind::Files),
            Err(Refusal::FilesFrozen)
        );
        assert_eq!(table.check_read("h_B", ReadKind::Files), Ok(()));
        // Seul le détenteur gèle ; la fin de l'opération dégèle.
        assert!(!table.set_files_frozen("h_A", "other", false));
        table.release_exclusive("h_A", "op1");
        assert_eq!(table.check_read("h_A", ReadKind::Files), Ok(()));
        assert!(
            !table.set_files_frozen("h_A", "op1", true),
            "plus de verrou"
        );
    }

    #[test]
    fn a_name_being_deleted_is_reserved_and_says_so() {
        let mut table = LockTable::new();
        table
            .reserve_name("terraria", "op1", OpKind::Delete)
            .unwrap();
        let refused = table
            .reserve_name("terraria", "op2", OpKind::Deploy)
            .unwrap_err();
        assert_eq!(refused, Refusal::NameReserved { by: OpKind::Delete });
        assert_eq!(refused.error_code(), ErrorCode::NameBeingDeleted);
        assert_eq!(table.name_reserved_by("terraria"), Some(OpKind::Delete));
        // Un nom réservé par un déploiement refuse aussi, avec un autre code.
        table.reserve_name("autre", "op3", OpKind::Deploy).unwrap();
        let refused = table
            .reserve_name("autre", "op4", OpKind::Deploy)
            .unwrap_err();
        assert_eq!(refused.error_code(), ErrorCode::OperationInProgress);
        assert!(!table.release_name("terraria", "op2"));
        assert!(table.release_name("terraria", "op1"));
        table
            .reserve_name("terraria", "op2", OpKind::Deploy)
            .unwrap();
    }

    #[test]
    fn heavy_io_has_a_single_seat_for_the_whole_server() {
        let mut table = LockTable::new();
        table.acquire_heavy_io("h_A", "op1").unwrap();
        let refused = table.acquire_heavy_io("h_B", "op2").unwrap_err();
        assert_eq!(
            refused,
            Refusal::HeavyIoBusy {
                service: "h_A".into()
            }
        );
        assert_eq!(refused.error_code(), ErrorCode::HeavyIoBusy);
        assert!(!table.release_heavy_io("op2"));
        assert!(table.release_heavy_io("op1"));
        table.acquire_heavy_io("h_B", "op2").unwrap();
    }

    #[test]
    fn the_end_of_an_operation_gives_back_everything_it_held() {
        let mut table = LockTable::new();
        table
            .acquire_exclusive("h_A", "op1", OpKind::Restore)
            .unwrap();
        table.reserve_name("a", "op1", OpKind::Restore).unwrap();
        table.acquire_heavy_io("h_A", "op1").unwrap();
        table.set_files_frozen("h_A", "op1", true);
        table.acquire_exclusive("h_B", "op9", OpKind::Stop).unwrap();
        table.release_all_of("op1");
        assert_eq!(table.held_services(), 1, "h_B reste pris par op9");
        assert_eq!(table.name_reserved_by("a"), None);
        table.acquire_heavy_io("h_Z", "op2").unwrap();
        assert_eq!(table.check_read("h_A", ReadKind::Files), Ok(()));
    }

    #[test]
    fn a_needs_attention_lock_is_rebuilt_at_startup_and_blocks_like_any_other() {
        let mut table = LockTable::new();
        table.restore_held("h_A", "op1", OpKind::Update).unwrap();
        assert!(table.acquire_shared_write("h_A").is_err());
        assert!(
            table
                .acquire_exclusive("h_A", "op2", OpKind::Start)
                .is_err()
        );
        // Deux opérations non terminées sur un même service ne se reconstruisent pas : la base l'interdit
        // aussi (index unique partiel), la table le refuse.
        assert!(table.restore_held("h_A", "op3", OpKind::Update).is_err());
    }

    #[test]
    fn refusals_are_all_conflicts() {
        for refusal in [
            Refusal::ServiceBusy {
                holder: Holder::FileWrites,
            },
            Refusal::NameReserved { by: OpKind::Deploy },
            Refusal::FilesFrozen,
        ] {
            assert_eq!(refusal.error_code().http_status(), 409);
        }
    }
}
