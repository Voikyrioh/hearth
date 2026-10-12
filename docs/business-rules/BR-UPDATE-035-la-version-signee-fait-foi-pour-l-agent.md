---
id: BR-UPDATE-035
domaine: UPDATE
titre: La version signée fait foi : l'agent refuse un binaire dont le commentaire signé n'annonce pas la version demandée
statut: active
invariant: true
source: HRT-48, contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md (sections 1, 9, 10.2), ADR-0021
maj: 2026-10-12
---

# BR-UPDATE-035 : La version signée fait foi

## Règle
La signature minisign d'un binaire de l'agent porte, dans son commentaire signé (« trusted comment »), un champ `version:X.Y.Z`. L'agent n'accepte une mise à jour que si ce champ est **égal** à la version demandée. Sinon la demande est refusée comme une signature invalide (`BAD_SIGNATURE`, « signature invalide » au journal d'activité, BR-UPDATE-024) :
- champ absent, en double, ou de forme non stricte (`v0.2.0`, `0.2.0-rc1`, `0.2`) ;
- version signée plus petite OU plus grande que la demandée.

Le refus a lieu **avant tout échange de binaire** et avant toute écriture : une première fois à la réception de la demande (commentaire lu sans le contenu, refus précoce : rien n'est téléchargé), puis de nouveau après le téléchargement, une fois la signature vérifiée (celle des octets ET la signature globale, qui couvre le commentaire) : seule cette seconde lecture fait foi, la première ne protège pas d'un commentaire réécrit. Dépôt, copie du superviseur et lancement ne viennent qu'après.

Pourquoi : sans cela, un ancien binaire authentique annoncé sous un numéro plus grand tournait en root jusqu'à 60 s avant d'être rejeté.

## Fabrication de la signature
La signature se fait à la main, avec la clé secrète du détenteur : `minisign -S -t "version:X.Y.Z" -s ~/.hearth/update.key -m hearth-agent -x hearth-agent.minisig` (option `-t` de minisign). Pas de commande `xtask` de signature (la clé secrète ne passe jamais par le dépôt ni la CI). `cargo xtask agent-manifest` REFUSE de publier une signature dont le commentaire ne porte pas `version:` égal à `--version` (même forme stricte que l'agent).

## Application (code)
- `crates/hearth-agent/src/domain/update/signed_version.rs::check_signed_version` (la règle, pure).
- `crates/hearth-agent/src/infrastructure/update/minisign.rs::MinisignVerifier::{check_format, verify}` (lecture du commentaire ; `verify` d'abord la signature globale).
- `crates/hearth-agent/src/application/update.rs::UpdateService::{start, execute}` (appel avant téléchargement, puis après).
- `xtask/src/release_core.rs::verify_agent_signature`, `xtask/src/agent_release.rs::run_manifest`.

## Vérification
- `domain/update/signed_version.rs` (tests unitaires : égale, absente, plus petite, plus grande, forme lâche ou double).
- `infrastructure/update/minisign.rs` : `a_signature_without_a_signed_version_is_refused_before_and_after_the_download`, `a_signed_version_other_than_the_requested_one_is_refused_smaller_and_greater`, `the_signed_comment_is_covered_by_the_global_signature`.
- `tests/update_use_cases.rs` : `a_signed_version_that_is_not_the_requested_one_is_refused_before_any_download`, `a_forged_signed_version_is_refused_after_the_download_and_before_any_write`.
- `tests/update_http.rs` : `a_signature_without_the_requested_version_is_refused_and_journaled_like_any_bad_signature`.
- `xtask/src/agent_release.rs` : `a_signature_without_the_version_or_with_another_one_is_never_published`.
- `deploy/e2e/scenario-update.sh` (signatures de `cargo xtask e2e-update` portent la version ; non rejoué à la livraison de HRT-48).

## Cas limites
- Le client fait l'équivalent pour son installateur (`requireSignedVersion`, BR-UPDATE-010). La 0.1.0 est la première à porter ce contrôle : c'est son vérificateur qui jugera la 0.1.1.
- Le refus de version porte le même code que les autres signatures refusées ; le détail (version signée, version demandée) va au journal technique de l'agent, jamais à l'écran.

## Règles liées
- BR-UPDATE-010, BR-UPDATE-024, ADR-0008, ADR-0021

## Historique
- 2026-10-12 : création (HRT-48).
