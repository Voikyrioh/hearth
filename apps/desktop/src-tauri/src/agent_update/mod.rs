//! Mise à jour de l'AGENT depuis le client (HRT-17, ADR-0021) : règles pures (`domain` : la cible
//! publiée dans le flux de versions, rien de rétrogradé, rien de non HTTPS ou de local), requête
//! typée et lecture des réponses (`wire`), types sérialisés (`dto`), cas d'usage (`service`) et
//! commandes (`commands`). La cible est lue par le client dans le flux de versions
//! (`update::feed::TauriFeed::check_agent`, dans la même tentative que la vérification du client) ;
//! la WebView ne fournit ni adresse, ni signature, ni somme. L'agent reste l'arbitre : rôle,
//! signature, somme, adresse.

pub mod commands;
pub mod domain;
pub mod dto;
pub mod service;
pub mod wire;
