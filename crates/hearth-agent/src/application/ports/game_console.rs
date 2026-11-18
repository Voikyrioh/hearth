//! Port `GameConsole` : un échange avec la console d'un serveur de jeu (HRT-267, ADR-0067, BR-CONSOLE-001, 007).
//!
//! L'adaptateur se connecte à l'adresse PRIVÉE du conteneur sur le réseau du moteur (`ContainerEngine::private_address`),
//! jamais à un port publié, envoie UNE ligne et lit la réponse, puis ferme. Le conteneur est hostile : la réponse est
//! bornée en taille et en temps. Le mot de passe est un `Secret` : il passe dans le paquet d'authentification, jamais
//! en argument d'un processus, jamais dans une trace ni une erreur. Le texte de la ligne et de la réponse ne sont ni
//! tracés ni gardés. Ce port n'exécute aucune commande dans le conteneur.

use std::net::SocketAddr;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::secret::Secret;

/// La réponse est coupée à cette taille (`truncated`).
pub const MAX_REPLY_BYTES: usize = 16 * 1024;

/// Les bornes d'un échange.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleBudget {
    /// Le délai de la connexion.
    pub connect: Duration,
    /// Le délai total de la réponse (`reply_timeout_s` de la fiche).
    pub reply: Duration,
    /// Fin de réponse : aucun octet pendant ce temps après le dernier paquet complet (Minecraft découpe une longue
    /// réponse en paquets sans marque de fin).
    pub quiet: Duration,
}

impl ConsoleBudget {
    pub const fn production(reply_s: u32) -> Self {
        Self {
            connect: Duration::from_secs(3),
            reply: Duration::from_secs(reply_s as u64),
            quiet: Duration::from_millis(250),
        }
    }
}

/// La réponse du jeu : du texte décodé et nettoyé (jamais écrit dans une trace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleReply {
    pub text: String,
    /// La réponse a été coupée à [`MAX_REPLY_BYTES`].
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ConsoleError {
    /// Connexion impossible, refusée, coupée, ou mot de passe refusé : la console est injoignable.
    #[error("console injoignable")]
    Unreachable,
    /// Le mot de passe est refusé (identifiant `-1` de la réponse d'authentification).
    #[error("mot de passe de la console refusé")]
    AuthRefused,
    /// Le jeu n'a pas répondu dans le délai : la commande est partie, son effet est inconnu.
    #[error("pas de réponse du jeu")]
    NoAnswer,
    /// Le jeu a parlé hors du protocole (longueur hors bornes, paquet incohérent) : coupé.
    #[error("réponse hors protocole")]
    Protocol,
}

#[async_trait]
pub trait GameConsole: Send + Sync {
    /// Se connecte à `address`, s'authentifie avec `secret`, envoie `line` et rend la réponse. La connexion est fermée
    /// à la fin de chaque échange.
    async fn exchange(
        &self,
        address: SocketAddr,
        secret: &Secret,
        line: &str,
        budget: ConsoleBudget,
    ) -> Result<ConsoleReply, ConsoleError>;
}
