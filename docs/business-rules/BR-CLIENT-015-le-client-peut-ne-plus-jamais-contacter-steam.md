---
id: BR-CLIENT-015
domaine: CLIENT
titre: Le client peut ne plus jamais contacter Steam
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (BR-CLIENT-015), HRT-239
maj: 2026-11-16
---

# BR-CLIENT-015 — Le réglage « Images des jeux depuis Steam »

## Règle
Le réglage « Images des jeux depuis Steam » (activé par défaut) se trouve dans la page Réglages de ce PC, avec une phrase qui dit ce que Steam voit (l'adresse internet de ce PC et l'identifiant du jeu demandé, rien d'autre). Désactivé, ce PC ne fait plus AUCUNE demande à Steam, oublie les images Steam qu'il gardait et montre le visuel neutre à la place ; l'illustration livrée avec l'agent (BR-FICHE-062) reste affichée, elle vient de votre serveur. Les images Steam gardées sur ce PC servent hors ligne.

Le réglage est appliqué par la COQUILLE, à chaque requête Steam (une page modifiée ne le contourne pas) ; une lecture impossible du réglage vaut « désactivé ». La page n'ouvre aucune connexion : elle affiche une `<img>` du protocole `hearth-illustration`. La coquille ne contacte que l'hôte compilé `shared.akamai.steamstatic.com`, en HTTPS, sans suivre aucune redirection, sans cookie, avec 10 secondes de délai, deux lectures au plus à la fois, 512 Kio au plus par image, `image/jpeg` exigé, signature et dimensions vérifiées. Cache : `catalog-illustrations/` du dossier de données local, 30 jours puis relecture conditionnelle, copie ancienne servie hors ligne, 32 Mio au plus.

## Application (code)
- `apps/desktop/src-tauri/src/illustrations/{mod,url,steam,cache}.rs`
- `apps/desktop/src-tauri/src/settings.rs::{remote_game_images, set_remote_game_images}`, `commands.rs::{get_remote_game_images, set_remote_game_images}`
- `apps/desktop/src-tauri/tauri.conf.json` (CSP : `img-src 'self' hearth-illustration: http://hearth-illustration.localhost`)
- `apps/desktop/src/stores/settings.ts`, `pages/Settings.vue` (la ligne du réglage)

## Vérification
- `illustrations/mod.rs::tests` : réglage coupé = aucune requête (le serveur de test compte zéro appel), cache 30 jours, `304`, Steam injoignable, adresse invalide `400`, méthode `405`, purge à la coupure
- `illustrations/steam.rs::tests` : redirection jamais suivie, corps de 513 Kio coupé, `text/html` et PNG refusés, dimensions hors bornes, relecture conditionnelle
- `illustrations/url.rs::tests`, `illustrations/cache.rs::tests`
- `apps/desktop/src-tauri/tests/illustrations_csp.rs` (aucun hôte externe, jamais `data:` en production)
- `apps/desktop/src/pages/Settings.test.ts`

## Règles liées
- BR-FICHE-003, BR-FICHE-062

## Historique
- 2026-11-16 — création (HRT-239).
