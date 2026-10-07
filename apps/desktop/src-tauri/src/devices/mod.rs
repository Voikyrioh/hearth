//! Postes de confiance depuis le client (HRT-23) : types sérialisés (`dto`), cas d'usage (`service`) et
//! commandes (`commands`). Une commande par action (ADR-0016) : le chemin et la preuve sont construits
//! par `hearth-link`, jamais par l'interface. La clé privée de ce PC ne passe JAMAIS ici : la
//! coquille ne la voit pas, la WebView encore moins (ADR-0023).

pub mod commands;
pub mod dto;
pub mod service;
