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

/// Une ligne reçue par le faux jeu.
#[derive(Debug, Clone)]
pub struct Received {
    pub address: SocketAddr,
    pub password: String,
    pub line: String,
}

enum Answer {
    Text(String),
    Truncated(String),
    Fail(ConsoleError),
}

pub struct FakeGameConsole {
    received: Mutex<Vec<Received>>,
    answers: Mutex<Vec<(String, Answer)>>,
    default: Mutex<String>,
    delay: Mutex<Duration>,
}

impl FakeGameConsole {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            received: Mutex::new(Vec::new()),
            answers: Mutex::new(Vec::new()),
            default: Mutex::new("ok".to_owned()),
            delay: Mutex::new(Duration::ZERO),
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
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Received {
                address,
                password: secret.expose().to_owned(),
                line: line.to_owned(),
            });
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
