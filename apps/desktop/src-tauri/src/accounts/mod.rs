//! Gestion des comptes depuis le client (HRT-13) : types sérialisés (`dto`), requêtes typées et
//! lecture des réponses, pures (`wire`), cas d'usage (`service`) et commandes (`commands`). Une
//! commande par action (ADR-0016) ; les règles de format viennent de `hearth-proto`
//! (`account_rules`), le contrôle d'accès est celui de l'agent.

pub mod commands;
pub mod dto;
pub mod service;
pub mod wire;
