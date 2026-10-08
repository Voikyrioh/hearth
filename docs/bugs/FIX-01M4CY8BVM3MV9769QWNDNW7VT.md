---
id: FIX-01M4CY8BVM3MV9769QWNDNW7VT
titre: Un pic d'une seconde disparaissait de l'historique d'une heure en vieillissant (moyenne par pas de 10 s)
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4CY8BVM3MV9769QWNDNW7VT : pic effacé par la moyenne de l'historique d'une heure

## Symptôme
Un pic à 100 % de processeur d'une seconde, visible en direct, redescendait à la moyenne de son pas de 10 s (environ 19 %) dès qu'il avait plus de cinq minutes, sur la fenêtre d'une heure.

## Reproduction
`domain::metrics::tests::the_peak_of_each_ten_second_step_keeps_a_one_second_spike_the_mean_hides` (la moyenne du pas est sous 25, le maximum est 100) ; `domain::history::tests::the_peaks_replace_the_means…` ; `tests/history_on_open.rs::the_peaks_of_the_agent_replace_the_means_in_the_hour_announced` ; `series.test.ts` « a peak carried by an old hour sample is drawn at its height ».

## Cause root
`/metrics/history?window=1h` ne rendait que la MOYENNE de chaque pas ; le maximum n'existait nulle part côté client pour ce qui avait plus de cinq minutes.

## Impacté
L'historique d'une heure du tableau de bord (HRT-18, PR #47), jamais publié.

## Workaround
Aucun.

## Correction
L'agent calcule le maximum de chaque pas (`domain::metrics::peaks`, même découpe que la moyenne) et le rend dans un champ AJOUTÉ `peaks` ; `hearth-link` le met à la place de la moyenne (`with_peaks`) ; le client trace déjà le maximum par pas. L'anneau en mémoire ne change pas de forme : les maxima sont calculés à la lecture, sur les mêmes échantillons bruts. `// FIX:01M4CY8BVM3MV9769QWNDNW7VT`.

## Règles
- BR-DASH-010, ADR-0015 §5, open-api metrics.

## Non-régression
- Les tests ci-dessus.

## Références
- Ticket : HRT-18 (suivi 1 des reviews #47)
- Code : `crates/hearth-agent/src/domain/metrics.rs`, `crates/hearth-link/src/domain/history.rs`
