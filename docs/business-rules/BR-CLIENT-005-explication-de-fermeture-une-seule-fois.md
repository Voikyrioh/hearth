---
id: BR-CLIENT-005
domaine: CLIENT
titre: L'explication de la réduction n'est donnée qu'à la première fermeture
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-installer-client.md (BR-CLIENT-005), HRT-08
maj: 2026-10-04
---

# BR-CLIENT-005 — L'explication de la réduction n'est donnée qu'à la première fermeture

## Règle
À la première fermeture, une notification discrète affiche « Hearth continue de fonctionner. Clique sur l'icône pour rouvrir. ». Le fait qu'elle ait été montrée est mémorisé (`closeHintSeen`, fichier `settings.json`) ; les fermetures suivantes n'affichent rien. La mémorisation n'a lieu que si la notification a bien été émise.

## Application (code)
- `apps/desktop/src-tauri/src/domain.rs::on_close_requested` (`HideAndExplain` seulement si `close_hint_seen` est faux) et `flag_from` (absent ou mal typé = faux).
- `apps/desktop/src-tauri/src/window.rs::explain_close` et `settings.rs::mark_close_hint_seen`.

## Vérification
- Tests : `tests/domain.rs::first_close_explains_then_stays_silent`, `a_missing_or_malformed_flag_defaults_to_false`.
- À la main : première croix = notification ; seconde = rien, y compris après redémarrage.

## Cas limites
- Lecture des réglages en échec : on considère l'explication déjà vue (pas de notification répétée).

## Règles liées
- BR-CLIENT-004

## Historique
- 2026-10-04 — création (HRT-08, session 2026-10-04-hearth-creation).
