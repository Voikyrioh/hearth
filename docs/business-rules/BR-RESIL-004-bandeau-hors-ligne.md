---
id: BR-RESIL-004
domaine: RESIL
titre: « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-lien-resilient.md (BR-RESIL-004), HRT-09
maj: 2026-10-05
---

# BR-RESIL-004 — « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant »

## Règle
Quand l'état du lien est « Hors ligne », un bandeau non modal affiche « Serveur hors ligne. Dernier contact à {heure}. Nouvelle tentative automatique en cours. » (heure locale `14h23`) et le bouton « Réessayer maintenant », qui demande une tentative immédiate (BR-RESIL-005). Sans contact connu, la phrase omet l'heure. Le bandeau disparaît au retour de « Connecté » ; en « Reconnexion… » il n'est pas affiché.

## Application (code)
- `apps/desktop/src/components/organisms/OfflineBanner.vue` ; `apps/desktop/src/layouts/ServerLayout.vue` (affiché si l'état est `offline`, `retry` → `link.retryNow`).
- `apps/desktop/src/stores/link.ts::retryNow` → `LinkBridge.retryNow`.

## Vérification
- Tests : `organisms.test.ts::OfflineBanner`, `shell.test.ts::goes connected -> reconnecting -> offline -> back`, `shell.test.ts::« Réessayer maintenant » asks the bridge to retry that server`, `e2e/shell.spec.ts`.

## Cas limites
- Texte exact : `fr.ts` clés `link.offlineBanner`, `link.offlineBannerNoContact`, `link.retryNow`.

## Règles liées
- BR-RESIL-001, BR-RESIL-005.

## Historique
- 2026-10-05 — création (HRT-09, session 2026-10-04-hearth-creation). Portée par l'interface ; le calcul des états est dans `hearth-link` (ADR-0007).
