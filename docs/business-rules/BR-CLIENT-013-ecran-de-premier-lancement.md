---
id: BR-CLIENT-013
domaine: CLIENT
titre: Sans serveur enregistré, l'application s'ouvre sur un écran d'accueil
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-013), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-013 — Sans serveur enregistré, l'application s'ouvre sur un écran d'accueil

## Règle
Premier lancement, aucun serveur : écran « Bienvenue dans Hearth », texte « Ajoute ton premier serveur pour commencer. », bouton « Ajouter un serveur ». Le bouton reste inactif, avec une infobulle, tant que l'assistant d'ajout n'existe pas. Aucune liste de serveurs n'est affichée.

## Application (code)
- `apps/desktop/src/pages/Welcome.vue` (textes dans `src/i18n/fr.ts`, clés `welcome.*`) ; page route `/welcome` dans `src/router/index.ts`.
- HORS DOMAIN : règle d'affichage portée par la vue ; la décision « aucun serveur » sera dans le store des serveurs (HRT à venir).

## Vérification
- Test : `src/pages/Welcome.test.ts`.

## Cas limites
- Dès qu'un serveur existe, l'écran n'apparaît plus (à câbler avec le carnet de serveurs).

## Règles liées
- BR-CLIENT-012

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
