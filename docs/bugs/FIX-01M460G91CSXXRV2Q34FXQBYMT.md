---
id: FIX-01M460G91CSXXRV2Q34FXQBYMT
titre: install.sh suivait une redirection de HTTPS vers HTTP avec wget
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M460G91CSXXRV2Q34FXQBYMT : install.sh suivait une redirection de HTTPS vers HTTP avec wget

## Symptôme
Avec `wget`, une adresse en HTTPS qui répond par une redirection vers HTTP était suivie : le binaire lancé en root se téléchargeait en clair.

## Cause root
`wget --https-only` ne vaut qu'en mode récursif ; `--max-redirect=5` suivait la redirection sans contrôler son schéma (`deploy/install.sh::download`).

## Impacté
Installation et désinstallation depuis HRT-15.

## Workaround
Aucun.

## Correction
`wget_https_only` : `--max-redirect=0`, les redirections sont suivies une à une et chacune doit être en HTTPS. Le scénario `deploy/e2e/scenario.sh` (redirection vers HTTP refusée) couvre le chemin ; `curl` avait déjà `--proto-redir '=https'`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-006 ; code : `deploy/install.sh` (marqueur `FIX:01M460G91CSXXRV2Q34FXQBYMT`)
