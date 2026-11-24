#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
//! Un faux jeu qui parle par la console (HRT-268) : il garde ce que l'agent lui a envoyé (adresse, mot de passe,
//! ligne) et répond selon un script. Aucune connexion réseau.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use hearth_agent::application::ports::{
    ConsoleBudget, ConsoleError, ConsoleReply, GameConsole, MAX_REPLY_BYTES,
};
use hearth_agent::domain::secret::Secret;

use super::fake_engine::{Call, FakeEngine};

/// Une ligne reçue par le faux jeu.
#[derive(Debug, Clone)]
pub struct Received {
    pub address: SocketAddr,
    pub password: String,
    pub line: String,
    /// Combien d'ordres d'arrêt ou de redémarrage le moteur avait déjà reçus quand la ligne est arrivée (0 sans
    /// `watch_engine`) : prouve qu'une ligne est partie AVANT l'arrêt (HRT-292).
    pub stops_seen: usize,
}

enum Answer {
    Text(String),
    Truncated(String),
    Fail(ConsoleError),
}

/// Début et fin (ou coupure) de chaque échange, en temps tokio.
type Spans = Arc<Mutex<Vec<(tokio::time::Instant, Option<tokio::time::Instant>)>>>;

pub struct FakeGameConsole {
    received: Mutex<Vec<Received>>,
    answers: Mutex<Vec<(String, Answer)>>,
    default: Mutex<String>,
    delay: Mutex<Duration>,
    engine: Mutex<Option<Arc<FakeEngine>>>,
    /// Pour chaque échange : son début et sa fin (ou sa coupure, quand l'appelant l'abandonne), en temps tokio.
    spans: Spans,
}

/// Note la fin d'un échange, même abandonné en route (le futur est jeté par un délai).
struct SpanEnd {
    spans: Spans,
    index: usize,
}

impl Drop for SpanEnd {
    fn drop(&mut self) {
        let mut spans = self.spans.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(span) = spans.get_mut(self.index) {
            span.1 = Some(tokio::time::Instant::now());
        }
    }
}

impl FakeGameConsole {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            received: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
            default: Mutex::new("ok".to_owned()),
            delay: Mutex::new(Duration::ZERO),
            engine: Mutex::new(None),
            spans: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Ce que le jeu répond à toute ligne sans réponse dédiée.
    pub fn answer_all(&self, text: &str) {
        *self.default.lock().unwrap_or_else(PoisonError::into_inner) = text.to_owned();
    }

    /// Ce que le jeu répond à cette ligne exacte.
    pub fn answer(&self, line: &str, text: &str) {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((line.to_owned(), Answer::Text(text.to_owned())));
    }

    pub fn truncated(&self, line: &str, text: &str) {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((line.to_owned(), Answer::Truncated(text.to_owned())));
    }

    /// Le jeu échoue à cette ligne.
    pub fn fail(&self, line: &str, error: ConsoleError) {
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((line.to_owned(), Answer::Fail(error)));
    }

    /// Le jeu met ce temps à répondre.
    pub fn delay(&self, delay: Duration) {
        *self.delay.lock().unwrap_or_else(PoisonError::into_inner) = delay;
    }

    /// Compte, à chaque ligne reçue, les arrêts déjà demandés au moteur.
    pub fn watch_engine(&self, engine: Arc<FakeEngine>) {
        *self.engine.lock().unwrap_or_else(PoisonError::into_inner) = Some(engine);
    }

    /// La durée de chaque échange commencé, de son début à sa fin ou à sa coupure ; `None` : encore en cours.
    pub fn durations(&self) -> Vec<Option<Duration>> {
        self.spans
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(start, end)| end.map(|end| end - *start))
            .collect()
    }

    pub fn received(&self) -> Vec<Received> {
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn lines(&self) -> Vec<String> {
        self.received().into_iter().map(|r| r.line).collect()
    }
}

#[async_trait]
impl GameConsole for FakeGameConsole {
    async fn exchange(
        &self,
        address: SocketAddr,
        secret: &Secret,
        line: &str,
        _budget: ConsoleBudget,
    ) -> Result<ConsoleReply, ConsoleError> {
        let stops_seen = self
            .engine
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map_or(0, |engine| {
                engine
                    .calls()
                    .iter()
                    .filter(|call| matches!(call, Call::Stop(..) | Call::Restart(..)))
                    .count()
            });
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Received {
                address,
                password: secret.expose().to_owned(),
                line: line.to_owned(),
                stops_seen,
            });
        let _span = {
            let mut spans = self.spans.lock().unwrap_or_else(PoisonError::into_inner);
            spans.push((tokio::time::Instant::now(), None));
            SpanEnd {
                spans: self.spans.clone(),
                index: spans.len() - 1,
            }
        };
        let delay = *self.delay.lock().unwrap_or_else(PoisonError::into_inner);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let scripted = {
            let answers = self.answers.lock().unwrap_or_else(PoisonError::into_inner);
            answers
                .iter()
                .rev()
                .find(|(l, _)| l == line)
                .map(|(_, a)| match a {
                    Answer::Text(text) => Answer::Text(text.clone()),
                    Answer::Truncated(text) => Answer::Truncated(text.clone()),
                    Answer::Fail(error) => Answer::Fail(*error),
                })
        };
        match scripted {
            Some(Answer::Text(text)) => Ok(ConsoleReply {
                text,
                truncated: false,
            }),
            Some(Answer::Truncated(text)) => Ok(ConsoleReply {
                text: text.chars().take(MAX_REPLY_BYTES).collect(),
                truncated: true,
            }),
            Some(Answer::Fail(error)) => Err(error),
            None => Ok(ConsoleReply {
                text: self
                    .default
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone(),
                truncated: false,
            }),
        }
    }
}
