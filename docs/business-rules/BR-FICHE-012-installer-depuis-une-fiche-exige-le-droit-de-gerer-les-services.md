---
id: BR-FICHE-012
domaine: FICHE
titre: Installer depuis une fiche exige le droit de gérer les services, quel que soit le rôle, et l'acte confirmé ; le serveur refuse sans lui même si le client est modifié
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-012), HRT-101
maj: 2026-10-24
---

# BR-FICHE-012 — Le droit d'installer

## Règle
`POST /deployments` avec un membre `source` est le même acte que le déploiement d'une image : niveau d'accès « services » (le droit « gérer les services », indépendant du rôle) et acte d'administration `ServiceDeploy` `0x0F` confirmé (preuve de clé du poste, mot de passe à chaque fois). Sans le droit, la couche d'accès refuse AVANT tout cas d'usage (`403`), ne crée ni service, ni description, ni dossier, et consigne le refus sous l'action `service.deploy` (BR-GRANT-004). La signature de l'acte porte `fiche:{id}` et l'empreinte `deploy_digest` (version et empreinte de la fiche, choix, identifiants des réponses, monde repris), jamais une réponse.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (POST /deployments, niveau Services)`
- `crates/hearth-agent/src/application/fiche_install.rs::FicheInstaller::prepare`
- `crates/hearth-proto/src/admin_act.rs::AdminAct::service_deploy`

## Vérification
- `crates/hearth-agent/tests/fiche_install_https.rs::an_account_without_the_right_creates_no_service`
- `crates/hearth-proto/src/admin_act.rs::a_fiche_source_signs_the_fiche_and_its_choices_through_the_digest_never_an_answer`

## Règles liées
- BR-FICHE-013, BR-DEPLOY-035

## Historique
- 2026-10-24 — création (HRT-101, étapes S6 et S9).
