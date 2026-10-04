//! Horloge du système.

use time::OffsetDateTime;

use crate::application::ports::Clock;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}
