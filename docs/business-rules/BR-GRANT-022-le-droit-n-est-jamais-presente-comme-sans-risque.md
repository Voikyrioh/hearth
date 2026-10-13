---
id: BR-GRANT-022
domaine: GRANT
titre: Le droit n'est jamais présenté comme sans risque : la documentation de sécurité dit ce que le droit donne et ce qu'il ne donne pas
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-022), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-022 — Le droit n'est jamais présenté comme sans risque : la documentation de sécurité dit ce que le droit donne et ce qu'il ne donne pas

## Règle
`SECURITY.md` (paragraphe « Le moteur de conteneurs ») dit : un compte qui a le droit choisit des images et les fait tourner sur le serveur ; il n'obtient pas les pouvoirs de l'administrateur du serveur par ce que Hearth crée, sauf si le serveur a ouvert un dossier sensible, sauf faille du moteur ou du système, et sauf pour un service adopté dont la configuration le permet déjà ; le processus de l'agent, lui, garde les pouvoirs de l'administrateur du serveur. HRT-76 réécrit ce paragraphe ; les garanties « conteneur borné » et « dossiers sur liste d'autorisation » qu'il annonce sont portées par des tickets suivants et le paragraphe le dit (état actuel : aucune barrière sur le contenu d'un conteneur n'est encore codée).

## Application (code)
- `SECURITY.md (paragraphe « Le moteur de conteneurs »)`

## Vérification
- `Relecture du paragraphe (règle documentaire, sans test automatique).`

## Cas limites
- Tant que le plan fermé et la liste d'autorisation ne sont pas livrés, le droit donne, en pratique, la main sur le serveur : le paragraphe l'écrit.

## Règles liées
- BR-GRANT-003, ADR-0041

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
