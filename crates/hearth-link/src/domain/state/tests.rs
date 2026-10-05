//! Chaque ligne du tableau des transitions de la spec (section 6) est un test nommé `rowNN_…` ;
//! les autres tests couvrent les seuils à la milliseconde, les tentatives et les cas limites.

use hearth_proto::error::UpgradeTarget;

use super::*;

/// Banc d'essai : une machine, une horloge simulée, un aléa nul (les délais sont exacts).
struct Rig {
    machine: LinkMachine,
    now: u64,
    log: Vec<(u64, Effect)>,
}

fn no_jitter() -> Box<dyn FnMut() -> u32 + Send> {
    // 200 sur 0..=400 : facteur 1,000, donc aucun aléa.
    Box::new(|| 200)
}

impl Rig {
    fn starting(start: Start) -> Self {
        Self {
            machine: LinkMachine::new(
                Thresholds::default(),
                no_jitter(),
                Mono::from_millis(0),
                start,
            ),
            now: 0,
            log: Vec::new(),
        }
    }

    /// Connectée depuis `t = 0` (premier contact établi).
    fn connected() -> Self {
        let mut rig = Self::starting(Start::Connecting);
        let effects = rig.send(Input::Tick);
        assert_eq!(effects, [Effect::StartAttempt]);
        let effects = rig.send(Input::Connected);
        assert_eq!(effects, [Effect::ResolvePending]);
        assert_eq!(rig.state(), LinkState::Connected);
        rig
    }

    fn send(&mut self, input: Input) -> Vec<Effect> {
        let effects = self.machine.handle(Mono::from_millis(self.now), input);
        for effect in &effects {
            self.log.push((self.now, *effect));
        }
        effects
    }

    /// Livre `input` à l'instant `at`. Tant que le lien est tenu, le serveur parle chaque seconde
    /// (métriques, battement) : c'est le cas normal. Pour tester le silence, on utilise
    /// `advance_to`, qui ne fait parler personne.
    fn send_at(&mut self, at: u64, input: Input) -> Vec<Effect> {
        self.heartbeat_until(at);
        self.advance_to(at);
        self.send(input)
    }

    fn heartbeat_until(&mut self, at: u64) {
        loop {
            let next = (self.now / 1_000 + 1) * 1_000;
            if next > at || !matches!(self.machine.phase, Phase::Up { .. }) {
                break;
            }
            self.now = next;
            self.send(Input::Traffic);
        }
    }

    /// Avance l'horloge en livrant un `Tick` à chaque échéance annoncée par la machine.
    fn advance_to(&mut self, target: u64) {
        for _ in 0..10_000 {
            match self.machine.deadline() {
                Some(deadline) if deadline.as_millis() <= target => {
                    self.now = self.now.max(deadline.as_millis());
                    self.send(Input::Tick);
                }
                _ => break,
            }
        }
        self.now = self.now.max(target);
    }

    fn state(&self) -> LinkState {
        self.machine.state()
    }

    fn since(&self) -> u64 {
        self.machine.status().since.as_millis()
    }

    fn count(&self, effect: Effect) -> usize {
        self.log.iter().filter(|(_, e)| *e == effect).count()
    }

    /// Simule un réseau mort : chaque tentative lancée échoue aussitôt, jusqu'à `until`.
    /// Rend les instants des tentatives.
    fn run_failing_until(&mut self, until: u64) -> Vec<u64> {
        let mut attempts = Vec::new();
        // Une tentative déjà lancée échoue d'abord.
        if matches!(self.machine.phase, Phase::Down(outage) if outage.in_flight) {
            self.send(Input::TransportFailed);
        }
        for _ in 0..100_000 {
            let Some(deadline) = self.machine.deadline() else {
                break;
            };
            if deadline.as_millis() > until {
                break;
            }
            self.now = self.now.max(deadline.as_millis());
            let effects = self.send(Input::Tick);
            if effects.contains(&Effect::StartAttempt) {
                attempts.push(self.now);
                self.send(Input::TransportFailed);
            }
        }
        self.now = self.now.max(until);
        attempts
    }
}

// ── Tableau des transitions (spec, section 6) ───────────────────────────────────────────────

#[test]
fn row01_connected_cut_under_3s_changes_nothing() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    assert_eq!(rig.state(), LinkState::Connected);
    rig.advance_to(12_999);
    assert_eq!(rig.state(), LinkState::Connected);
    rig.send_at(12_999, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
    assert_eq!(
        rig.machine.status().since,
        Mono::from_millis(0),
        "aucun changement affiché"
    );
}

#[test]
fn row02_connected_cut_between_3s_and_30s_shows_reconnecting() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.advance_to(12_999);
    assert_eq!(
        rig.state(),
        LinkState::Connected,
        "2 999 ms : encore invisible"
    );
    rig.advance_to(13_000);
    assert_eq!(
        rig.state(),
        LinkState::Reconnecting,
        "3 000 ms : « Reconnexion en cours »"
    );
    assert_eq!(rig.since(), 13_000);
}

#[test]
fn row03_connected_cut_over_30s_shows_offline() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.advance_to(39_999);
    assert_eq!(rig.state(), LinkState::Reconnecting, "29 999 ms");
    rig.advance_to(40_000);
    assert_eq!(
        rig.state(),
        LinkState::Offline,
        "30 000 ms : « Hors ligne »"
    );
    assert_eq!(rig.since(), 40_000);
}

#[test]
fn row04_reconnecting_then_link_back_before_30s_is_connected() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.advance_to(20_000);
    assert_eq!(rig.state(), LinkState::Reconnecting);
    let effects = rig.send_at(20_000, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
    assert_eq!(effects, [Effect::ResolvePending]);
}

#[test]
fn row05_reconnecting_with_no_answer_for_30s_goes_offline() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    let attempts = rig.run_failing_until(39_999);
    assert!(
        attempts.len() >= 5,
        "les tentatives se poursuivent : {attempts:?}"
    );
    assert_eq!(rig.state(), LinkState::Reconnecting);
    rig.run_failing_until(40_000);
    assert_eq!(rig.state(), LinkState::Offline);
}

#[test]
fn row06_offline_retry_now_shows_reconnecting_and_starts_an_attempt() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.run_failing_until(60_000);
    assert_eq!(rig.state(), LinkState::Offline);
    let effects = rig.send_at(60_001, Input::RetryNow);
    assert_eq!(effects, [Effect::StartAttempt]);
    assert_eq!(rig.state(), LinkState::Reconnecting);
    // L'échec ramène à « Hors ligne » (BR-RESIL, section 3).
    rig.send_at(60_050, Input::TransportFailed);
    assert_eq!(rig.state(), LinkState::Offline);
}

#[test]
fn row07_offline_link_back_automatically_is_connected() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.run_failing_until(90_000);
    assert_eq!(rig.state(), LinkState::Offline);
    // La prochaine tentative réussit.
    let deadline = rig
        .machine
        .deadline()
        .map(Mono::as_millis)
        .unwrap_or(90_000);
    let before = rig.count(Effect::StartAttempt);
    rig.advance_to(deadline);
    assert_eq!(rig.count(Effect::StartAttempt), before + 1);
    let effects = rig.send(Input::Connected);
    assert_eq!(effects, [Effect::ResolvePending]);
    assert_eq!(rig.state(), LinkState::Connected);
}

#[test]
fn row08_server_dead_for_ten_minutes_stays_offline_and_keeps_trying() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    let attempts = rig.run_failing_until(10_000 + 10 * 60 * 1_000);
    assert_eq!(rig.state(), LinkState::Offline);
    assert!(
        rig.machine.deadline().is_some(),
        "les tentatives n'ont pas de fin"
    );
    // Espacement maximal : 30 s entre deux tentatives.
    let gaps: Vec<u64> = attempts.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.iter().all(|gap| *gap <= 30_000), "{gaps:?}");
    assert!(
        gaps.iter().rev().take(10).all(|gap| *gap == 30_000),
        "{gaps:?}"
    );
    assert_eq!(rig.machine.status().blocked, None);
}

#[test]
fn row09_connected_session_expired_without_saved_password_shows_session_expired() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(5_000, Input::SessionExpired { can_reauth: false });
    assert_eq!(rig.state(), LinkState::SessionExpired);
    assert_eq!(
        effects,
        [
            Effect::AbortAttempt,
            Effect::CloseStream,
            Effect::MarkPendingUnknown
        ]
    );
    // Pas de boucle de reconnexion (BR-RESIL-012).
    assert_eq!(rig.machine.deadline(), None);
    assert!(rig.run_failing_until(10 * 60 * 1_000).is_empty());
    assert_eq!(rig.state(), LinkState::SessionExpired);
}

#[test]
fn row10_session_expired_then_password_accepted_is_connected() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::SessionExpired { can_reauth: false });
    let effects = rig.send_at(8_000, Input::LoginSucceeded);
    assert!(effects.contains(&Effect::StartAttempt));
    assert_eq!(rig.state(), LinkState::Reconnecting);
    rig.send_at(8_300, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
}

#[test]
fn row11_session_expired_then_password_refused_stays_session_expired() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::SessionExpired { can_reauth: false });
    let effects = rig.send_at(8_000, Input::LoginRefused);
    assert!(effects.is_empty());
    assert_eq!(rig.state(), LinkState::SessionExpired);
}

#[test]
fn row12_connected_account_revoked_shows_access_revoked_and_stops_trying() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(5_000, Input::AccessRevoked);
    assert_eq!(rig.state(), LinkState::AccessRevoked);
    assert!(effects.contains(&Effect::AbortAttempt));
    assert_eq!(rig.machine.deadline(), None);
    assert!(
        rig.run_failing_until(3_600_000).is_empty(),
        "aucune tentative après une révocation"
    );
    // Ni un déclencheur ne relance (pas de reconnexion avec les anciens identifiants).
    assert!(rig.send(Input::RetryNow).is_empty());
    assert!(rig.send(Input::Woke).is_empty());
    assert!(rig.send(Input::NetworkChanged).is_empty());
    assert_eq!(rig.state(), LinkState::AccessRevoked);
}

#[test]
fn row13_access_revoked_then_another_account_is_connected() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::AccessRevoked);
    let effects = rig.send_at(9_000, Input::LoginSucceeded);
    assert!(effects.contains(&Effect::StartAttempt));
    rig.send_at(9_200, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
}

#[test]
fn row14_offline_wake_or_network_change_shows_reconnecting_and_attempts_at_once() {
    for trigger in [Input::Woke, Input::NetworkChanged] {
        let mut rig = Rig::connected();
        rig.send_at(10_000, Input::TransportFailed);
        rig.run_failing_until(100_000);
        assert_eq!(rig.state(), LinkState::Offline);
        let effects = rig.send_at(100_010, trigger);
        assert_eq!(effects, [Effect::StartAttempt], "{trigger:?}");
        assert_eq!(rig.state(), LinkState::Reconnecting, "{trigger:?}");
    }
}

#[test]
fn row15_agent_alone_restarting_recovers_in_a_few_seconds() {
    let mut rig = Rig::connected();
    // L'agent s'arrête : le flux se ferme à t = 10 s ; il est de retour à t = 14 s.
    rig.send_at(10_000, Input::TransportFailed);
    let attempts = rig.run_failing_until(13_999);
    assert_eq!(
        rig.state(),
        LinkState::Reconnecting,
        "au-delà de 3 s : « Reconnexion en cours »"
    );
    assert!(attempts.len() >= 3, "{attempts:?}");
    // La première tentative qui part après le retour de l'agent aboutit.
    let deadline = rig
        .machine
        .deadline()
        .map(Mono::as_millis)
        .unwrap_or(14_000);
    rig.advance_to(deadline);
    rig.send(Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
    assert!(
        deadline - 10_000 < 10_000,
        "retour en {} ms",
        deadline - 10_000
    );
}

// ── Silence (battement) ─────────────────────────────────────────────────────────────────────

#[test]
fn silence_of_exactly_3s_cuts_the_link_and_it_is_dated_at_the_last_message() {
    let mut rig = Rig::connected();
    rig.send_at(1_000, Input::Traffic);
    rig.advance_to(3_999);
    assert_eq!(rig.state(), LinkState::Connected);
    assert_eq!(
        rig.count(Effect::CloseStream),
        0,
        "2 999 ms de silence : lien tenu"
    );
    rig.advance_to(4_000);
    assert_eq!(
        rig.count(Effect::CloseStream),
        1,
        "3 000 ms de silence : lien coupé"
    );
    assert_eq!(rig.count(Effect::MarkPendingUnknown), 1);
    // Silence de 3 s depuis le dernier message : coupure de 3 s, donc déjà « Reconnexion ».
    assert_eq!(rig.state(), LinkState::Reconnecting);
    assert_eq!(rig.since(), 4_000);
}

#[test]
fn traffic_pushes_the_silence_deadline_back() {
    let mut rig = Rig::connected();
    for second in 1..=10 {
        rig.send_at(second * 1_000, Input::Traffic);
    }
    assert_eq!(rig.state(), LinkState::Connected);
    assert_eq!(rig.count(Effect::CloseStream), 0);
    assert_eq!(rig.machine.deadline(), Some(Mono::from_millis(13_000)));
    assert_eq!(
        rig.machine.status().last_contact,
        Some(Mono::from_millis(10_000))
    );
}

#[test]
fn a_cut_found_by_an_error_starts_at_the_error_not_at_the_last_message() {
    let mut rig = Rig::connected();
    rig.send_at(2_000, Input::Traffic);
    rig.send_at(2_900, Input::TransportFailed);
    rig.advance_to(5_899);
    assert_eq!(rig.state(), LinkState::Connected);
    rig.advance_to(5_900);
    assert_eq!(rig.state(), LinkState::Reconnecting);
}

// ── Tentatives ──────────────────────────────────────────────────────────────────────────────

#[test]
fn the_first_attempt_is_immediate_then_delays_follow_the_sequence() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(10_000, Input::TransportFailed);
    assert_eq!(
        effects,
        [
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
            Effect::StartAttempt
        ]
    );
    // Attente de la réponse de la première tentative : aucune échéance de tentative.
    assert_eq!(rig.machine.status().next_retry_at, None);
    let mut at = 10_000;
    for delay in [500, 1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 30_000] {
        rig.send_at(at, Input::TransportFailed);
        assert_eq!(
            rig.machine.status().next_retry_at,
            Some(Mono::from_millis(at + delay)),
            "après l'échec à {at}"
        );
        at += delay;
        let before = rig.count(Effect::StartAttempt);
        rig.advance_to(at - 1);
        assert_eq!(rig.count(Effect::StartAttempt), before, "pas avant {at}");
        rig.advance_to(at);
        assert_eq!(rig.count(Effect::StartAttempt), before + 1, "à {at}");
    }
}

#[test]
fn a_success_restarts_the_delays_from_half_a_second() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.run_failing_until(40_000);
    assert!(rig.machine.status().failed_attempts >= 5);
    let deadline = rig
        .machine
        .deadline()
        .map(Mono::as_millis)
        .unwrap_or(41_000);
    rig.advance_to(deadline);
    rig.send(Input::Connected);
    assert_eq!(rig.machine.status().failed_attempts, 0);
    rig.send_at(deadline + 5_000, Input::TransportFailed);
    rig.send_at(deadline + 5_001, Input::TransportFailed);
    assert_eq!(
        rig.machine.status().next_retry_at,
        Some(Mono::from_millis(deadline + 5_001 + 500))
    );
}

#[test]
fn jitter_is_applied_and_bounded_by_the_cap() {
    let mut machine = LinkMachine::new(
        Thresholds::default(),
        Box::new(|| u32::MAX),
        Mono::from_millis(0),
        Start::Connecting,
    );
    machine.handle(Mono::from_millis(0), Input::Tick);
    let mut now = 0;
    for _ in 0..12 {
        machine.handle(Mono::from_millis(now), Input::TransportFailed);
        let next = machine
            .status()
            .next_retry_at
            .map(Mono::as_millis)
            .unwrap_or(0);
        assert!(next - now <= 30_000, "{}", next - now);
        now = next;
        machine.handle(Mono::from_millis(now), Input::Tick);
    }
}

#[test]
fn a_trigger_while_an_attempt_is_running_restarts_it() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.advance_to(10_500);
    let effects = rig.send(Input::NetworkChanged);
    assert_eq!(effects, [Effect::StartAttempt]);
}

#[test]
fn ticks_never_start_a_second_attempt_while_one_is_running() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    // Aucun échec n'est rapporté : la première tentative est toujours en cours.
    rig.advance_to(120_000);
    assert_eq!(
        rig.count(Effect::StartAttempt),
        2,
        "démarrage + une seule tentative de reprise"
    );
    assert_eq!(rig.state(), LinkState::Offline);
}

#[test]
fn a_late_tick_dates_the_state_at_the_threshold_not_at_the_late_instant() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    // Le tick n'arrive qu'à 50 s (processus gelé) : l'état « Hors ligne » date de 40 s.
    rig.now = 50_000;
    rig.send(Input::Tick);
    assert_eq!(rig.state(), LinkState::Offline);
    assert_eq!(rig.since(), 40_000);
}

#[test]
fn a_stale_failure_without_a_running_attempt_does_not_advance_the_delays() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.send_at(10_001, Input::TransportFailed);
    let before = rig.machine.status();
    rig.send_at(10_002, Input::TransportFailed);
    assert_eq!(rig.machine.status(), before);
}

// ── Réveil et réseau depuis « Connecté » ────────────────────────────────────────────────────

#[test]
fn waking_while_connected_counts_the_outage_from_the_last_message() {
    let mut rig = Rig::connected();
    rig.send_at(2_000, Input::Traffic);
    // Le PC dormait depuis 2 s ; il se réveille à 100 s.
    rig.now = 100_000;
    let effects = rig.send(Input::Woke);
    assert_eq!(
        effects,
        [
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
            Effect::StartAttempt
        ]
    );
    assert_eq!(
        rig.state(),
        LinkState::Offline,
        "98 s de coupure : directement « Hors ligne »"
    );
}

#[test]
fn a_network_change_while_connected_reopens_the_link_quietly() {
    let mut rig = Rig::connected();
    rig.send_at(2_000, Input::Traffic);
    let effects = rig.send_at(2_100, Input::NetworkChanged);
    assert_eq!(
        effects,
        [
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
            Effect::StartAttempt
        ]
    );
    assert_eq!(
        rig.state(),
        LinkState::Connected,
        "invisible tant que moins de 3 s"
    );
    rig.send_at(2_400, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
}

#[test]
fn retry_now_while_connected_does_nothing() {
    let mut rig = Rig::connected();
    assert!(rig.send_at(5_000, Input::RetryNow).is_empty());
}

// ── Session expirée avec mot de passe mémorisé (BR-RESIL-013) ───────────────────────────────

#[test]
fn an_expired_session_with_a_saved_password_reconnects_silently() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(5_000, Input::SessionExpired { can_reauth: true });
    assert_eq!(
        effects,
        [
            Effect::CloseStream,
            Effect::MarkPendingUnknown,
            Effect::Reauthenticate
        ]
    );
    assert_eq!(rig.state(), LinkState::Connected, "rien à l'écran");
    let effects = rig.send_at(5_300, Input::Reauthenticated);
    assert_eq!(effects, [Effect::StartAttempt]);
    rig.send_at(5_600, Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
    assert_eq!(rig.machine.status().since, Mono::from_millis(0));
}

#[test]
fn a_silent_reconnection_that_hits_refused_credentials_means_access_revoked() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::SessionExpired { can_reauth: true });
    rig.send_at(5_200, Input::AccessRevoked);
    assert_eq!(rig.state(), LinkState::AccessRevoked);
    assert_eq!(rig.machine.deadline(), None);
}

#[test]
fn a_silent_reconnection_that_cannot_reach_the_server_keeps_the_reauth_step() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::SessionExpired { can_reauth: true });
    rig.send_at(5_100, Input::TransportFailed);
    let before = rig.count(Effect::Reauthenticate);
    rig.advance_to(5_600);
    assert_eq!(rig.count(Effect::Reauthenticate), before + 1);
}

#[test]
fn a_session_already_expired_does_not_become_expired_again_or_revoked_by_stale_events() {
    let mut rig = Rig::connected();
    rig.send_at(5_000, Input::SessionExpired { can_reauth: false });
    assert!(rig.send(Input::AccessRevoked).is_empty());
    assert_eq!(rig.state(), LinkState::SessionExpired);
    assert!(rig.send(Input::TransportFailed).is_empty());
    assert!(rig.send(Input::Connected).is_empty());
    assert_eq!(rig.state(), LinkState::SessionExpired);
}

// ── Empreinte changée, versions incompatibles ───────────────────────────────────────────────

#[test]
fn a_changed_fingerprint_blocks_every_attempt() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    let effects = rig.send_at(10_100, Input::FingerprintChanged);
    assert!(effects.contains(&Effect::AbortAttempt));
    let status = rig.machine.status();
    assert_eq!(status.state, LinkState::Offline);
    assert_eq!(status.blocked, Some(Blocked::FingerprintChanged));
    assert_eq!(status.next_retry_at, None);
    assert_eq!(rig.machine.deadline(), None);
    assert!(rig.run_failing_until(600_000).is_empty());
    assert!(rig.send(Input::Woke).is_empty());
    assert!(rig.send(Input::NetworkChanged).is_empty());
}

#[test]
fn only_an_explicit_retry_lifts_a_block() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::FingerprintChanged);
    let effects = rig.send_at(20_000, Input::RetryNow);
    assert_eq!(effects, [Effect::StartAttempt]);
    assert_eq!(rig.machine.status().blocked, None);
    assert_eq!(rig.state(), LinkState::Reconnecting);
}

#[test]
fn incompatible_versions_block_and_say_who_updates() {
    let mut rig = Rig::connected();
    rig.send_at(1_000, Input::Incompatible(UpgradeTarget::Agent));
    let status = rig.machine.status();
    assert_eq!(
        status.blocked,
        Some(Blocked::IncompatibleVersion(UpgradeTarget::Agent))
    );
    assert_eq!(status.state, LinkState::Offline);
    assert_eq!(rig.machine.deadline(), None);
}

// ── Départs, déconnexion, arrêt ─────────────────────────────────────────────────────────────

#[test]
fn starting_with_a_saved_session_shows_reconnecting_and_attempts_at_once() {
    let mut rig = Rig::starting(Start::Connecting);
    assert_eq!(rig.state(), LinkState::Reconnecting);
    assert_eq!(rig.machine.deadline(), Some(Mono::from_millis(0)));
    assert_eq!(rig.send(Input::Tick), [Effect::StartAttempt]);
    // Jamais « Connecté » tant que rien n'a répondu.
    rig.advance_to(2_000);
    assert_eq!(rig.state(), LinkState::Reconnecting);
    rig.run_failing_until(30_000);
    assert_eq!(rig.state(), LinkState::Offline);
}

#[test]
fn starting_without_a_session_waits_for_the_user() {
    let rig = Rig::starting(Start::SignedOut);
    assert_eq!(rig.state(), LinkState::SessionExpired);
    assert_eq!(rig.machine.deadline(), None);
}

#[test]
fn after_an_internal_incident_the_link_restarts_offline_and_retries() {
    let mut rig = Rig::starting(Start::Recovered);
    assert_eq!(rig.state(), LinkState::Offline);
    assert_eq!(rig.send(Input::Tick), [Effect::StartAttempt]);
    rig.send(Input::Connected);
    assert_eq!(rig.state(), LinkState::Connected);
}

#[test]
fn logging_out_stops_everything_and_shows_session_expired() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(5_000, Input::LoggedOut);
    assert_eq!(
        effects,
        [
            Effect::AbortAttempt,
            Effect::CloseStream,
            Effect::MarkPendingUnknown
        ]
    );
    assert_eq!(rig.state(), LinkState::SessionExpired);
    assert_eq!(rig.machine.deadline(), None);
}

#[test]
fn shutdown_ends_everything_and_later_events_are_ignored() {
    let mut rig = Rig::connected();
    let effects = rig.send_at(5_000, Input::Shutdown);
    assert_eq!(
        effects,
        [Effect::AbortAttempt, Effect::CloseStream, Effect::Stop]
    );
    for input in [
        Input::Tick,
        Input::Traffic,
        Input::Connected,
        Input::TransportFailed,
        Input::RetryNow,
        Input::LoginSucceeded,
        Input::Woke,
    ] {
        assert!(rig.send(input).is_empty(), "{input:?}");
    }
}

#[test]
fn a_connection_that_arrives_after_logout_is_ignored() {
    let mut rig = Rig::connected();
    rig.send_at(10_000, Input::TransportFailed);
    rig.send_at(10_100, Input::LoggedOut);
    assert!(rig.send(Input::Connected).is_empty());
    assert_eq!(rig.state(), LinkState::SessionExpired);
}

#[test]
fn every_input_in_every_phase_is_handled_without_panicking_or_spinning() {
    let inputs = [
        Input::Connected,
        Input::Traffic,
        Input::Tick,
        Input::TransportFailed,
        Input::SessionExpired { can_reauth: true },
        Input::SessionExpired { can_reauth: false },
        Input::Reauthenticated,
        Input::AccessRevoked,
        Input::FingerprintChanged,
        Input::Incompatible(UpgradeTarget::Client),
        Input::RetryNow,
        Input::Woke,
        Input::NetworkChanged,
        Input::LoginSucceeded,
        Input::LoginRefused,
        Input::LoggedOut,
    ];
    // Suite pseudo-aléatoire déterministe : la machine ne doit jamais paniquer, et ses échéances
    // doivent toujours être consommables (pas de boucle active).
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut rig = Rig::starting(Start::Connecting);
    for _ in 0..20_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let input = inputs[(seed % inputs.len() as u64) as usize];
        let step = (seed >> 20) % 4_000;
        rig.now += step;
        rig.send(input);
        // Une échéance dans le passé doit disparaître après un Tick.
        if let Some(deadline) = rig.machine.deadline()
            && deadline.as_millis() <= rig.now
        {
            rig.send(Input::Tick);
            if let Some(again) = rig.machine.deadline() {
                assert!(
                    again.as_millis() > rig.now || rig.machine.status().next_retry_at.is_some(),
                    "échéance non consommée : {again:?} à {}",
                    rig.now
                );
            }
        }
    }
}
