---
id: FIX-01M46G800Z47XQ8R64G2MDC4NP
titre: « Se souvenir » restait coché sans mot de passe au coffre après une application tuée pendant l'ajout
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M46G800Z47XQ8R64G2MDC4NP : « se souvenir » sans secret

## Symptôme
Application tuée pendant l'ajout d'un serveur (après l'écriture du carnet, avant celle du mot de passe au coffre) : au lancement suivant, le carnet dit « se souvenir » mais le coffre est vide. Le formulaire de reconnexion montre la case cochée, et le serveur est présenté comme « identifiants mémorisés » alors qu'aucune reconnexion silencieuse n'est possible.

## Cause root
`LinkManager::start` rechargeait le carnet tel quel : `ServerRecord::remember` n'était jamais recoupé avec le coffre.

## Impacté
Depuis HRT-10, après un arrêt brutal pendant l'ajout ou si le coffre a été vidé à la main.

## Workaround
Aucun.

## Correction
À l'ouverture, un serveur marqué `remember` dont le coffre répond « aucun mot de passe » (`Ok(None)`, pas une erreur du coffre) repasse à `remember = false`, en mémoire et sur disque. Test `tracking.rs::a_remembered_login_without_a_password_in_the_vault_is_not_promised_at_startup`. La correction est dans la bibliothèque (source de vérité), pas dans l'interface.

## Références
- Ticket : HRT-12 (suivi de la review de HRT-10, PR #12)
- BR : BR-CONN-004
