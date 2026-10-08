---
id: FIX-01M4D6KNC8G4T68J25J7DXMMNM
titre: Le client retombait en silence sur les moyennes quand les maxima manquaient
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D6KNC8G4T68J25J7DXMMNM : Le client retombait en silence sur les moyennes quand les maxima manquaient

## Symptôme
Si l'agent ne rendait pas `peaks` (agent d'avant, route qui cesse d'en rendre) ou en rendait un nombre différent des échantillons, la courbe d'une heure montrait les moyennes sans que rien ne le dise : la correction des pics pouvait s'éteindre sans rouge.

## Reproduction
`history::tests::the_fit_of_the_peaks_tells_a_missing_or_short_list_so_it_can_be_logged` ; `tests/http_api.rs::the_hour_window_route_hands_back_one_peak_per_sample_and_short_windows_none` (rouge si la route cesse de rendre `peaks`).

## Cause root
`with_peaks` rendait les échantillons tels quels sans le dire.

## Impacté
Agent et client depuis la tranche concernée (jamais publié).

## Workaround
Aucun.

## Correction
`peaks_fit` dit si les maxima s'appliquent, manquent ou ont la mauvaise longueur ; `manager/attempt.rs::read_hour` journalise (serveur, tailles, jamais le jeton) UNE fois par exécution et par serveur : `info` quand les maxima sont simplement absents (agent plus ancien légitime), `warn` seulement quand ils sont présents mais de mauvaise longueur (`first_note`). Un test HTTP prouve que la route rend un maximum par échantillon sur l'heure et aucun sur les fenêtres à une seconde. `FIX:` dans `history.rs` et `attempt.rs`.

## Règles
- BR-DASH-010 (le client lit l'heure à la connexion), BR-RESIL-008 (le client ne cache pas un repli) ; ADR-0015 §5.

## Non-régression
- Les deux tests ci-dessus.

## Références
- Ticket : HRT-18 (suites des reviews des PR #42 et #48)
