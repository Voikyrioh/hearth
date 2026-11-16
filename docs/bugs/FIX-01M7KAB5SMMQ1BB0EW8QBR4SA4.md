---
id: FIX-01M7KAB5SMMQ1BB0EW8QBR4SA4
titre: Les images Steam : réglage coupé pendant la lecture, client sans TLS, écritures simultanées et JPEG coupé (HRT-239)
date_découverte: 2026-11-17
date_correction: 2026-11-17
---

# FIX-01M7KAB5SMMQ1BB0EW8QBR4SA4 : Protocole des illustrations (HRT-239)

Sévérité : majeur (confidentialité du réglage) et mineur (robustesse).

## Symptôme
Un réglage « Images des jeux depuis Steam » coupé pendant une lecture déjà partie laissait l'image écrite dans le cache et servie ; un échec de configuration TLS donnait un client par défaut (redirections suivies, autre TLS) ; deux écritures simultanées du même fichier partageaient un nom temporaire ; un JPEG coupé mais à l'en-tête valable était accepté.

## Reproduction
Couper le réglage pendant la réponse de Steam ; écrire quatre fois en parallèle la même image ; servir un JPEG tronqué avant son marqueur de fin.

## Cause root
Le réglage n'était relu qu'avant la requête ; `build` retombait sur `Client::default` ; le nom temporaire était `.<nom>.tmp` ; `check_image` ne lisait que l'en-tête.

## Impacté
`src-tauri/src/illustrations/{mod,steam,cache}.rs`.

## Workaround
Aucun.

## Correction
Le réglage est relu après la lecture et avant toute écriture (404 s'il est coupé) ; `SteamClient.http` est un `Option` (aucun client sûr : `Unreachable`, un avertissement) ; nom temporaire unique (pid et compteur atomique) ; `check_image` exige la fin `FF D9` ; la lecture conditionnelle envoie aussi `If-Modified-Since`.

## Références
- HRT-239, BR-FICHE-062
