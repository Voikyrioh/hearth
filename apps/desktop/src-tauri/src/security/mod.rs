//! Sécurité d'un serveur depuis le client (HRT-26, ADR-0025) : l'alerte « attaque probable », le mode
//! attaque et l'état de ce poste. `dto` (types sérialisés, JAMAIS une clé, un défi, une signature ni un
//! jeton), `book` (l'état tenu côté Rust, numéro de séquence par serveur, rejoué à l'abonnement,
//! ADR-0013 point 3), `service` (cas d'usage, lecture des réponses par code d'erreur stable) et
//! `commands` (une commande par action, aucun paramètre libre, ADR-0016). La clé de ce PC reste dans
//! `hearth-link` : la coquille ne la voit pas, la WebView encore moins.

pub mod book;
pub mod commands;
pub mod dto;
pub mod service;
