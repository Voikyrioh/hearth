//! Envoyer un fichier dans la zone d'attente de l'agent depuis le client (HRT-96, HRT-95, ADR-0048, `BR-FILES-019`,
//! `BR-MOD-015`) : `dto` (types sérialisés), `service` (le carnet des envois, l'exécution en tâche de fond, la lecture
//! du fichier local, l'interprétation de l'ouverture d'un dépôt) et `commands` (la boîte « Ouvrir » du système, démarrer,
//! lister, reprendre, annuler, retirer de la liste). La page ne reçoit qu'un chemin au départ et des nombres ensuite :
//! la coquille lit le fichier elle-même.

pub mod commands;
pub mod dto;
pub mod service;
