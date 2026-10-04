//! Points d'entrée : API HTTPS, ligne de commande, signaux. Traduction et contrôle d'accès,
//! pas de règle métier. Ne dépend que d'`application` et `domain` : l'assemblage est fait par
//! `app.rs`, jamais d'import de `infrastructure`.

pub mod account;
pub mod cli;
pub mod http;
pub mod signal;
pub mod terminal;
