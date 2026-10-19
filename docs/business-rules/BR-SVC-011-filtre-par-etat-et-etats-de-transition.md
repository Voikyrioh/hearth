---
id: BR-SVC-011
domaine: SVC
titre: Le filtre par état ne montre une transition que sous « Tous », et « Modification en cours » compte en transition
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-011), contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md, HRT-84
maj: 2026-10-17
---

# BR-SVC-011 — Le filtre par état ne montre une transition que sous « Tous »

## Règle
Le filtre par état propose « Tous », « En marche », « Arrêté », « En erreur ». « En erreur » regroupe « En erreur » et « Redémarre en boucle ». Un service en transition (« Démarrage », « Arrêt en cours », « Déploiement en cours », « Modification en cours »), un service « En pause » et un service « État non reconnu » ne sont visibles que sous « Tous », sauf qu'un service dont une action de l'utilisateur est en cours reste visible sous le filtre actif jusqu'à la fin de l'action. Les compteurs du moteur (BR-ENGINE-004) rangent « Modification en cours » dans « en transition ». Le dixième état `updating` est une valeur additive de `ServiceState` : un client plus ancien la lit « inconnue ».

## Application (code)
- `apps/desktop/src/services/counts.ts::categoryOf`
- `apps/desktop/src/services/counts.ts::visibleUnder`
- `crates/hearth-proto/src/api/services.rs::ServiceState`

## Vérification
- `apps/desktop/src/services/transition.test.ts`
- `apps/desktop/src/pages/ServicesOperations.test.ts` (compteur « 2 en transition »)

## Cas limites
- Le filtre lui-même (barre d'outils, nombres par filtre) arrive avec HRT-82 : `visibleUnder` est la règle qu'il consommera.

## Historique
- 2026-10-17 — création (HRT-84, dixième état et règle de visibilité sous le filtre).
