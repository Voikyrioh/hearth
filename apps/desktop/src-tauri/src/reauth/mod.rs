//! La confirmation des actes d'administration depuis le client (HRT-30, ADR-0033, BR-TRUST-036, 042,
//! 043) : ce que l'agent annonce (capacité, réglage du compte, élévation de 5 minutes), si ce PC a une clé
//! au coffre, si un acte est couvert par l'élévation, et le réglage « Demander mon mot de passe ». `dto`
//! (types sérialisés, JAMAIS un mot de passe, une clé, un défi ni une signature), `wire` (lecture des
//! refus de confirmation par code d'erreur stable, partagée par toutes les actions), `service` et
//! `commands` (une commande par action, aucun paramètre libre, ADR-0016).

pub mod commands;
pub mod dto;
pub mod service;
pub mod wire;
