//! Le moteur de conteneurs et les services d'un serveur depuis le client (HRT-62, ADR-0038), en lecture
//! seule : `dto` (types sérialisés, copie fidèle de `hearth_proto::api::services`, SANS valeur ni nom de
//! variable d'environnement et sans commande de lancement), `book` (le dernier état par serveur, numéro
//! de séquence, rejoué à l'abonnement, ADR-0013 point 3) et `commands` (une commande par geste, aucun
//! paramètre libre). Les lectures de journaux (HRT-67) : `logs` (une session par onglet, bornée, sans rien garder) et `logs_dto`. Les règles (capacité de l'agent, fusion des lots) sont dans `hearth-link`.

pub mod book;
pub mod commands;
pub mod dto;
pub mod logs;
pub mod logs_dto;
