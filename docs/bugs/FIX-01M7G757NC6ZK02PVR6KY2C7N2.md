---
id: FIX-01M7G757NC6ZK02PVR6KY2C7N2
titre: Toute installation depuis une fiche est refusée par le moteur en quelques millisecondes (image épinglée par empreinte mal découpée)
date_découverte: 2026-11-15
date_correction: 2026-11-15
---

# FIX-01M7G757NC6ZK02PVR6KY2C7N2 : Image épinglée par empreinte mal découpée au téléchargement

Sévérité : critique (aucune fiche épinglée, donc ni Minecraft ni Terraria, ne s'installait quand l'image n'était pas déjà sur le serveur).

## Symptôme
« Le moteur a refusé de créer le service. Rien n'a été créé. » Journal : `déploiement terminé status Failed cause engine_refused duration_ms 4` : aucun téléchargement réel.

## Cause root
La référence passée au moteur d'une fiche est `dépôt:version@sha256:…` (`domain/services/plan.rs:119-120`). Le téléchargement la découpait avec `split_image` (`hearth-proto/src/api/service_spec.rs:36-46`), qui coupe au DERNIER « : » : celui de `sha256:`. Le moteur recevait `fromImage = dépôt:version@sha256` et `tag = <hex>`, une référence invalide (`400`). `translate_pull` ne reconnaît pas ce texte et retombe sur `translate` : `Refused`, donc `engine_refused`. `image_present` (qui donne la référence entière à `inspect_image`) répond « absente » sur un serveur neuf, ce qui mène toujours au téléchargement ; la suite de tests, dont le faux moteur ignore le découpage, ne le voyait pas. Cause établie par lecture du code et test unitaire (le texte exact du moteur n'a pas été relevé sur la forge : écritures sur le moteur non autorisées dans cette session).

## Reproduction
Installer une fiche épinglée (Minecraft) sur un serveur dont l'image n'est pas présente. Test : `infrastructure/engine/bollard.rs::tests::a_pinned_image_is_pulled_whole_and_an_unpinned_one_by_repository_and_tag`.

## Impacté
`infrastructure/engine/bollard.rs` (`pull`), donc aussi la modification d'un service par recréation (`service_update.rs`).

## Workaround
Avoir déjà l'image épinglée sur le serveur (`docker pull dépôt@sha256:…` puis étiqueter), non testé.

## Correction
`pull_target` : une référence qui contient `@` est donnée entière à `create_image` (`fromImage`) sans `tag` ; sans empreinte, comportement inchangé. Alternative écartée : changer `split_image` (partagé avec la signature de l'acte `deploy_digest`, dont l'empreinte ne doit pas bouger).

## Références
- Code : marqueur `FIX:01M7G757NC6ZK02PVR6KY2C7N2`
