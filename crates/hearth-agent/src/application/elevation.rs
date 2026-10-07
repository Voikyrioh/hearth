//! L'élévation du mot de passe (HRT-28, Q19, BR-TRUST-042, 043) : après une saisie juste, le mot de passe
//! n'est plus demandé pendant 5 minutes pour les actes que `domain::trust::admin_act::covered_by_elevation`
//! couvre. **Tenue par l'agent, en mémoire** : jamais en base (elle ne survit pas à un redémarrage du
//! service), jamais côté client. Horloge monotone : reculer l'heure ne la prolonge pas. Non glissante.
//!
//! Une entrée par session, liée à la session, au poste de la clé prouvée et à l'adresse. Bornée : la plus
//! ancienne sort quand la table est pleine (au pire le mot de passe est redemandé).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use super::ports::MonotonicClock;
use crate::domain::accounts::AccountId;
use crate::domain::known_address::canonical;
use crate::domain::sessions::SessionId;
use crate::domain::trust::DeviceId;
use crate::domain::trust::admin_act::{elevation_holds, elevation_remaining_s};

/// Nombre d'élévations tenues au plus.
pub const MAX_ELEVATIONS: usize = 1_024;

struct Entry {
    account: AccountId,
    device: DeviceId,
    addr: String,
    opened_ms: u64,
}

#[derive(Default)]
struct State {
    entries: HashMap<SessionId, Entry>,
    /// Ordre d'ouverture, pour faire sortir la plus ancienne.
    order: VecDeque<SessionId>,
}

pub struct Elevations {
    monotonic: Arc<dyn MonotonicClock>,
    state: Mutex<State>,
}

impl Elevations {
    pub fn new(monotonic: Arc<dyn MonotonicClock>) -> Self {
        Self {
            monotonic,
            state: Mutex::new(State::default()),
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.monotonic.elapsed().whole_milliseconds()).unwrap_or(0)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ouvre (ou relance : une nouvelle saisie du mot de passe) l'élévation de cette session.
    pub fn open(&self, session: &SessionId, account: &AccountId, device: &DeviceId, addr: &str) {
        let now = self.now_ms();
        let mut state = self.lock();
        if state.entries.remove(session).is_some() {
            state.order.retain(|id| id != session);
        }
        while state.entries.len() >= MAX_ELEVATIONS {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            state.entries.remove(&oldest);
        }
        state.order.push_back(session.clone());
        state.entries.insert(
            session.clone(),
            Entry {
                account: account.clone(),
                device: device.clone(),
                addr: canonical(addr),
                opened_ms: now,
            },
        );
    }

    /// L'élévation couvre-t-elle cette requête : même session, même poste (celui de la clé prouvée),
    /// même adresse, et pas expirée ? Une entrée expirée est oubliée.
    pub fn covers(&self, session: &SessionId, device: &DeviceId, addr: &str) -> bool {
        let now = self.now_ms();
        let mut state = self.lock();
        let Some(entry) = state.entries.get(session) else {
            return false;
        };
        if !elevation_holds(entry.opened_ms, now) {
            state.entries.remove(session);
            state.order.retain(|id| id != session);
            return false;
        }
        entry.device == *device && entry.addr == canonical(addr)
    }

    /// Secondes restantes de l'élévation de cette session depuis cette adresse, 0 sinon (indicatif).
    pub fn remaining_s(&self, session: &SessionId, addr: &str) -> u64 {
        let now = self.now_ms();
        let state = self.lock();
        state
            .entries
            .get(session)
            .filter(|entry| entry.addr == canonical(addr))
            .map_or(0, |entry| elevation_remaining_s(entry.opened_ms, now))
    }

    fn close_where(&self, keep: impl Fn(&SessionId, &Entry) -> bool) {
        let mut state = self.lock();
        state.entries.retain(|session, entry| keep(session, entry));
        let State { entries, order } = &mut *state;
        order.retain(|session| entries.contains_key(session));
    }

    /// Fin de la session (déconnexion, fermeture, révocation).
    pub fn close_session(&self, session: &SessionId) {
        self.close_where(|id, _| id != session);
    }

    /// Mot de passe ou rôle changé, compte supprimé, premier mot de passe faux, réglage remis à `each` :
    /// toutes les élévations du compte.
    pub fn close_account(&self, account: &AccountId) {
        self.close_where(|_, entry| entry.account != *account);
    }

    /// Poste retiré ou oublié.
    pub fn close_device(&self, device: &DeviceId) {
        self.close_where(|_, entry| entry.device != *device);
    }

    /// Mode attaque actif ou suspendu : aucune élévation n'existe.
    pub fn close_all(&self) {
        self.close_where(|_, _| false);
    }

    /// Nombre d'élévations tenues (tests).
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use time::Duration;

    use super::*;

    struct Fake(AtomicU64);

    impl MonotonicClock for Fake {
        fn elapsed(&self) -> Duration {
            Duration::milliseconds(i64::try_from(self.0.load(Ordering::SeqCst)).unwrap())
        }
    }

    fn store() -> (Elevations, Arc<Fake>) {
        let clock = Arc::new(Fake(AtomicU64::new(1_000)));
        (Elevations::new(clock.clone()), clock)
    }

    fn ids() -> (SessionId, AccountId, DeviceId) {
        (
            SessionId::new("S1"),
            AccountId::new("A1"),
            DeviceId::new("D1"),
        )
    }

    #[test]
    fn an_elevation_covers_its_session_device_and_address_for_five_minutes() {
        let (store, clock) = store();
        let (session, account, device) = ids();
        assert!(!store.covers(&session, &device, "10.0.0.1"));
        store.open(&session, &account, &device, "10.0.0.1");
        assert!(store.covers(&session, &device, "10.0.0.1"));
        assert!(!store.covers(&SessionId::new("S2"), &device, "10.0.0.1"));
        assert!(!store.covers(&session, &DeviceId::new("D2"), "10.0.0.1"));
        assert!(!store.covers(&session, &device, "10.0.0.2"));
        clock.0.fetch_add(299_999, Ordering::SeqCst);
        assert!(store.covers(&session, &device, "10.0.0.1"));
        assert_eq!(store.remaining_s(&session, "10.0.0.1"), 1);
        clock.0.fetch_add(1, Ordering::SeqCst);
        assert!(!store.covers(&session, &device, "10.0.0.1"));
        assert!(store.is_empty(), "une entrée expirée est oubliée");
    }

    #[test]
    fn using_an_elevation_does_not_extend_it_only_a_new_password_restarts_it() {
        let (store, clock) = store();
        let (session, account, device) = ids();
        store.open(&session, &account, &device, "10.0.0.1");
        clock.0.fetch_add(200_000, Ordering::SeqCst);
        assert!(store.covers(&session, &device, "10.0.0.1"));
        clock.0.fetch_add(101_000, Ordering::SeqCst);
        assert!(
            !store.covers(&session, &device, "10.0.0.1"),
            "non glissante"
        );
        store.open(&session, &account, &device, "10.0.0.1");
        clock.0.fetch_add(200_000, Ordering::SeqCst);
        assert!(store.covers(&session, &device, "10.0.0.1"));
        assert_eq!(store.len(), 1, "une seule entrée par session");
    }

    #[test]
    fn each_cause_of_closure_removes_exactly_what_it_should() {
        let (store, _) = store();
        let a = (
            SessionId::new("S1"),
            AccountId::new("A1"),
            DeviceId::new("D1"),
        );
        let b = (
            SessionId::new("S2"),
            AccountId::new("A2"),
            DeviceId::new("D2"),
        );
        let open_both = |store: &Elevations| {
            store.open(&a.0, &a.1, &a.2, "10.0.0.1");
            store.open(&b.0, &b.1, &b.2, "10.0.0.2");
        };
        open_both(&store);
        store.close_session(&a.0);
        assert!(!store.covers(&a.0, &a.2, "10.0.0.1") && store.covers(&b.0, &b.2, "10.0.0.2"));
        open_both(&store);
        store.close_account(&a.1);
        assert!(!store.covers(&a.0, &a.2, "10.0.0.1") && store.covers(&b.0, &b.2, "10.0.0.2"));
        open_both(&store);
        store.close_device(&b.2);
        assert!(store.covers(&a.0, &a.2, "10.0.0.1") && !store.covers(&b.0, &b.2, "10.0.0.2"));
        open_both(&store);
        store.close_all();
        assert!(store.is_empty());
    }

    #[test]
    fn the_table_is_bounded_and_the_oldest_leaves_first() {
        let (store, _) = store();
        let (account, device) = (AccountId::new("A"), DeviceId::new("D"));
        for index in 0..=MAX_ELEVATIONS {
            store.open(
                &SessionId::new(format!("S{index}")),
                &account,
                &device,
                "10.0.0.1",
            );
        }
        assert_eq!(store.len(), MAX_ELEVATIONS);
        assert!(!store.covers(&SessionId::new("S0"), &device, "10.0.0.1"));
        assert!(store.covers(
            &SessionId::new(format!("S{MAX_ELEVATIONS}")),
            &device,
            "10.0.0.1"
        ));
    }
}
