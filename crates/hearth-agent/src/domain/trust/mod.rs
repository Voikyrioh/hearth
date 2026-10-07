//! Identité d'appareil : le défi, l'inscription d'un poste, l'oubli des postes (HRT-22,
//! BR-TRUST-003 à 005, 007, 022 à 026). Fonctions pures : le temps est un paramètre, la
//! cryptographie et le stockage sont des ports de l'application.

pub mod attack_mode;
pub mod challenge;
pub mod device;
pub mod recognition;
pub mod weak_key;

pub use challenge::{
    Challenge, ConsumedChallenges, MAX_CONSUMED, TTL_MS, check as check_challenge, mac_input,
};
pub use device::{
    DeviceId, Enrollment, NewDevice, RETENTION, TrustedDevice, cutoff, device_name,
    judge_enrollment,
};
pub use recognition::{
    LoginCriteria, LoginStanding, Mode, SessionStanding, TrialKind, judge_login, judge_session,
    mode_of,
};
pub use weak_key::has_small_order;
