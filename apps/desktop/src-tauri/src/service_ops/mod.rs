//! Le suivi des opérations de service depuis le client (HRT-84, ADR-0045) : `dto` (types sérialisés, copie
//! fidèle de `hearth_proto::api::service_ops`, rien de sensible), `book` (les opérations en cours de
//! chaque serveur et les issues pas encore annoncées, rejouées à l'abonnement, ADR-0013 point 3),
//! `service` (la décision, un acte confirmé), `lifecycle` (démarrer, arrêter, redémarrer : trois actes
//! confirmés, HRT-65), `deploy` et `deploy_dto` (déployer une image, annuler son opération, consulter le formulaire et
//! lire les bornes de la machine, HRT-69) et `commands` (lister, acquitter, décider, piloter ; aucune commande n'annule ni ne crée une opération).

pub mod book;
pub mod commands;
pub mod deploy;
pub mod deploy_dto;
pub mod dto;
pub mod lifecycle;
pub mod service;
