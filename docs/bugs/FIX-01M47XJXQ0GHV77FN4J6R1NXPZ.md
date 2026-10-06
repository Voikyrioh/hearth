---
id: FIX-01M47XJXQ0GHV77FN4J6R1NXPZ
titre: Le filtre d'adresses du téléchargement se contournait avec HTTPS_PROXY (BR-UPDATE-027)
date_découverte: 2026-10-06
date_correction: 2026-10-06
---

# FIX-01M47XJXQ0GHV77FN4J6R1NXPZ : `HTTPS_PROXY` contourne le filtre d'adresses

## Symptôme
Avec `HTTPS_PROXY` (ou `HTTP_PROXY`, `ALL_PROXY`) dans l'environnement du service, le client de téléchargement passait par le proxy. Un administrateur (ou un jeton volé) pouvait faire joindre un nom interne par le proxy : la règle BR-UPDATE-027 (jamais d'adresse locale ou privée) ne s'appliquait plus.

## Reproduction
Test rouge avant correctif : `tests/update_download.rs::an_environment_proxy_is_never_used_so_the_address_filter_cannot_be_bypassed` (le test se relance avec les variables de proxy posées vers un faux proxy qui compte les connexions ; avant : le téléchargement partait vers le proxy et échouait en `Unreachable`).

## Cause root
`reqwest::Client::builder()` lit les proxys de l'environnement par défaut. Avec un proxy, le nom de l'hôte part au proxy, qui le résout lui-même : le résolveur dédié (`PublicOnlyResolver`) et le contrôle après résolution ne voient rien.

## Impacté
Production (agent root) : une règle invariante contournable. Conditions : un proxy d'environnement dans l'unité du service, ou posé par un administrateur.

## Workaround
Aucun proxy dans l'environnement du service.

## Correction
`.no_proxy()` sur le client : aucune variable de proxy n'est lue. Conséquence assumée : un serveur qui ne sort que par un proxy sortant ne peut pas se mettre à jour à distance (`unreachable`) ; mise à jour par `install.sh --binary`.

## Règles
- BR-UPDATE-027 : précisée (aucun proxy d'environnement).

## Non-régression
- `tests/update_download.rs::an_environment_proxy_is_never_used_so_the_address_filter_cannot_be_bypassed` (variables en majuscules et en minuscules : `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY`, `http_proxy`, `ALL_PROXY`, `all_proxy`).

## Références
- Ticket : HRT-17 (suivis de review, PR #21), tâche T27
- Code : `crates/hearth-agent/src/infrastructure/update/download.rs` (marqueur `FIX:01M47XJXQ0GHV77FN4J6R1NXPZ`)
