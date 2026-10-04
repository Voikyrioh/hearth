//! Points d'entrée : API HTTPS, ligne de commande, signaux. Traduction et contrôle d'accès,
//! pas de règle métier. Ne dépend que d'`application` : l'assemblage est fait par `app.rs`.

pub mod cli;
pub mod http;
pub mod signal;
