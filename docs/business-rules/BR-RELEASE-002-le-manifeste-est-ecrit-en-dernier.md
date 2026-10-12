---
id: BR-RELEASE-002
domaine: RELEASE
titre: Le manifeste est écrit en dernier, et seulement si la relecture publique de tous les fichiers est bonne
statut: active
invariant: true
source: HRT-51, contexts/hearth/conceptions/2026-10-09-distribution-scaleway.md (sections 4.3, 8.2)
maj: 2026-10-12
---

# BR-RELEASE-002 : Le manifeste est écrit en dernier

## Règle
Ordre du dépôt (`release-upload --confirm`), jamais un autre :
1. les 8 fichiers de la version (`releases/X.Y.Z/*`) ;
2. la relecture de chacun par `https://hearth.voikyrioh.fr/api/releases/X.Y.Z/{fichier}` (Cloudflare, Traefik, API, bucket) et la comparaison de sa somme SHA-256, avec une patience de 5 minutes (un tour toutes les 10 s) ;
3. `releases/install.sh` ;
4. `releases/manifest.json`, EN DERNIER, un seul objet : l'annonce est atomique.

Si une relecture échoue encore au bout des 5 minutes (injoignable, 404, somme différente), ni le script ni le manifeste ne sont écrits : rien n'est annoncé, on relance (les fichiers déjà déposés à l'identique sont passés, BR-RELEASE-001).

Pourquoi : un client ne doit jamais voir un manifeste qui cite un fichier absent ou différent.

L'essai à blanc est le comportement par défaut : sans `--confirm`, la commande affiche chaque écriture prévue (clé, somme, taille) et ne lance aucune commande externe.

## Application (code)
- `xtask/src/release_publish.rs::publish`, `::read_back`, `::plan`.

## Vérification
- `xtask/src/release_publish.rs` : `the_writes_come_in_the_order_of_the_conception_with_the_manifest_last`, `the_dry_run_shows_every_write_and_calls_nothing`, `when_the_public_readback_fails_the_manifest_and_the_script_are_not_written`, `a_wrong_sum_on_the_public_readback_means_no_manifest`, `the_readback_waits_for_a_slow_cache_then_succeeds`.

## Cas limites
- La lecture de `GET /api/releases/latest` après dépôt n'est pas faite par l'outil (cache de 5 minutes de Cloudflare) : elle reste à la main.
- L'ordre de la conception 8.2 place le script après la relecture et avant le manifeste : suivi tel quel.

## Règles liées
- BR-RELEASE-001, BR-RELEASE-003

## Historique
- 2026-10-12 : création (HRT-51).
