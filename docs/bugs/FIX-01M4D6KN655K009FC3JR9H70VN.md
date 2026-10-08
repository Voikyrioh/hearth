---
id: FIX-01M4D6KN655K009FC3JR9H70VN
titre: La température tracée sur l'heure restait une moyenne
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4D6KN655K009FC3JR9H70VN : La température tracée sur l'heure restait une moyenne

## Symptôme
Sur la fenêtre d'une heure la température (sondes et carte graphique) est tracée, mais l'agent ne rendait un maximum par pas que pour le processeur, la mémoire, les débits et la charge : un coup de chaud d'une seconde disparaissait dans la moyenne de 10 s. La doc de `StepPeak` disait « chaque mesure tracée », ce qui était faux.

## Reproduction
`domain::metrics::tests::the_peak_of_a_step_keeps_the_hottest_second_of_each_probe_and_of_the_gpu` (rouge avant : pas de champ).

## Cause root
`peaks` ne calculait pas de maximum pour `temps` ni pour `temp_c` des cartes.

## Impacté
Agent et client depuis la tranche concernée (jamais publié).

## Workaround
Aucun.

## Correction
`StepPeak.temps` (une entrée par sonde) et `GpuPeak.temp_c` : champs AJOUTÉS (rien de retiré, absents chez un agent d'avant, ignorés d'un client d'avant) ; le client les applique par nom de sonde. Deux sondes homonymes ne se mélangent plus (reconnues par nom ET rang, moyenne comme maximum : `two_probes_with_the_same_name_are_never_mixed`). Documentation de `StepPeak` remise d'équerre : les cœurs et les disques ne sont pas tracés sur l'heure et gardent leur moyenne. `FIX:` dans `domain/metrics.rs`.

## Règles
- BR-DASH-010 (l'heure écoulée et ses maxima, fiche à compléter de la température), BR-DASH-006 (températures) ; ADR-0015 §5.

## Non-régression
- Agent `domain::metrics::tests` (température maximale), proto `the_peaks_are_an_added_field…` (lecteur d'avant joué pour de bon, pic d'avant sans les champs), liaison `history::tests::the_peaks_replace_the_means…`.

## Références
- Ticket : HRT-18 (suites des reviews des PR #42 et #48)
