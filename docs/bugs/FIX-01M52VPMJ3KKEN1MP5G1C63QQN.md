---
id: FIX-01M52VPMJ3KKEN1MP5G1C63QQN
titre: La preuve d'absence d'un objet refusait la ligne d'erreur préfixée des AWS CLI récents (HRT-171)
date_découverte: 2026-10-14
date_correction: 2026-10-16
---

# FIX-01M52VPMJ3KKEN1MP5G1C63QQN : `proves_absence` n'acceptait pas `aws: [ERROR]: ` (HRT-171)

## Symptôme
Au premier dépôt réel (0.1.0), `cargo xtask release-upload --confirm` s'arrêtait avec « l'état de … dans le bucket est impossible à établir », sans rien écrire, alors que le bucket répondait bien 404 pour une clé absente.

## Reproduction
`xtask/src/release_publish.rs`, tests `the_prefixed_404_line_of_a_recent_aws_cli_is_an_absence` et `only_the_exact_prefix_is_accepted_before_the_404_line`. En réel : `aws s3api head-object` sur une clé absente avec AWS CLI 2.38.1.

## Cause root
`proves_absence` exigeait une ligne EXACTEMENT égale à la phrase 404. Relevé contre Scaleway (fr-par) : AWS CLI 2.27.0 écrit la phrase seule, AWS CLI 2.38.1 la fait précéder de `aws: [ERROR]: `. La phrase attendue avait été supposée, non vérifiée.

## Impacté
Tout dépôt avec un AWS CLI récent (arrêt sûr, rien d'écrit). Contourné avec AWS CLI 2.27.0.

## Workaround
Installer AWS CLI 2.27.0 (`scoop install aws@2.27.0`, `scoop hold aws`).

## Correction
La ligne est acceptée seule ou précédée exactement de `ABSENT_PREFIX` (`aws: [ERROR]: `). Rien d'autre : autre préfixe, espace en plus, texte après, autre code de sortie que 254 restent des refus (tests « 403 », « délai contenant 404 », « 500 … not found » inchangés). Commentaire « supposée, non vérifiée » remplacé par le relevé.

## Références
- Ticket : HRT-171
