//! Le suivi des opérations de service depuis le client (HRT-84, ADR-0045) : `dto` (types sérialisés, copie
//! fidèle de `hearth_proto::api::service_ops`, rien de sensible), `book` (les opérations en cours de
//! chaque serveur et les issues pas encore annoncées, rejouées à l'abonnement, ADR-0013 point 3),
//! `service` (la décision, un acte confirmé) et `commands` (lister, acquitter, décider ; aucune commande
//! n'annule ni ne lance une opération).

pub mod book;
pub mod commands;
pub mod dto;
pub mod service;
