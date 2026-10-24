#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Faux hasard déterministe pour les tests d'une installation depuis une fiche (HRT-101) : une suite d'octets
//! reproductible, ou une panne du générateur. Aucun test ne tire du vrai hasard.

use std::sync::{Mutex, PoisonError};

use hearth_agent::application::ports::{RandomError, SecretRandom};

pub struct FakeRandom {
    next: Mutex<u8>,
    failing: bool,
}

impl FakeRandom {
    /// Les octets `start`, `start + 1`, ... (modulo 256), dans l'ordre des demandes.
    pub fn counting_from(start: u8) -> Self {
        Self {
            next: Mutex::new(start),
            failing: false,
        }
    }

    /// Un générateur du système indisponible.
    pub fn failing() -> Self {
        Self {
            next: Mutex::new(0),
            failing: true,
        }
    }
}

impl SecretRandom for FakeRandom {
    fn fill(&self, buf: &mut [u8]) -> Result<(), RandomError> {
        if self.failing {
            return Err(RandomError("panne simulée".to_owned()));
        }
        let mut next = self.next.lock().unwrap_or_else(PoisonError::into_inner);
        for byte in buf.iter_mut() {
            *byte = *next;
            *next = next.wrapping_add(1);
        }
        Ok(())
    }
}
