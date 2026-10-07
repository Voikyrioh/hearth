---
id: BR-TRUST-022
domaine: TRUST
titre: Un compte a jusqu'à 8 postes inscrits, le 9e n'est pas inscrit sans rien supprimer ; le titulaire voit la liste et en retire un
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-06-fonctionnelle-poste-de-confiance.md ; conception technique 2026-10-06 ; contexts/hearth/tickets/hrt/HRT-22.md ; ADR-0023
maj: 2026-10-07
---

# BR-TRUST-022 : Un compte a jusqu'à 8 postes inscrits, le 9e n'est pas inscrit sans rien supprimer ; le titulaire voit la liste et en retire un

## Règle
Un compte peut avoir jusqu'à **8** postes inscrits. Un poste n'est inscrit que par une connexion réussie avec le mot de passe, jamais par une session seule (BR-TRUST-004). Quand le compte a déjà 8 postes, le 9e n'est **pas** inscrit : la connexion réussit quand même (`device: "limit"`), **sans suppression automatique** d'un poste existant. Le titulaire voit la liste de ses postes (`GET /me/devices`) et peut en retirer un (`DELETE /me/devices/{id}`) ; le poste retiré peut ensuite être inscrit de nouveau par une connexion par mot de passe.

- **Retirer un poste est un acte d'administration** (Q16, 2026-10-07 : « administration = mdp (plus tard 2fa) et clé priv ») : `DELETE /me/devices/{id}` exige, en plus de la session, (1) la **preuve de possession de la clé du poste COURANT** (le poste relié à la session à la connexion, jamais réécrit : BR-TRUST-048 ; inscrite pour le compte ; défi d'usage `0x04` « retrait », lié au jeton et à l'identifiant du poste visé) et (2) le **mot de passe actuel**, vérifié par le chemin de la connexion (mêmes compteurs, même ralentissement : `429` après cinq échecs). La preuve est vérifiée d'abord : une session volée, seule, n'essaie aucun mot de passe et ne retire rien ; avec le mot de passe mais sans la clé, rien non plus ; la preuve d'un autre compte, d'une autre session, ou signée pour un autre poste visé ne vaut rien ; une preuve rejouée est refusée ; le défi n'est consommé que si le retrait réussit. Une session sans poste inscrit (client ancien) reçoit `422 VALIDATION_ERROR`, `details.reason = device_required` (explicite : l'utilisateur est authentifié). **Voies de secours** quand aucun poste inscrit ne permet de retirer : changement du mot de passe du compte par un administrateur (`PUT /accounts/{id}/password`) ou par `hearth-agent account passwd` sur le serveur (oublient tous les postes et adresses, BR-TRUST-024), `hearth-agent account revoke`. Lister reste permis à la session seule.
- La liste ne montre que les postes de l'appelant : nom, dates, dernière adresse, et lequel est « courant » ; jamais une clé, une empreinte de clé, un défi.
- Retirer un poste retire aussi **son adresse retenue et ses sessions** (accès révoqué) : un portable perdu ne garde pas une session vivante. Sont fermées les sessions **liées au poste** (ouvertes avec sa clé ou prouvées par elle) ET **toutes** celles du même compte **sans lien à un poste**, sauf la session courante (HRT-24, suivi de la revue de la PR #25 : une session sans lien ouverte depuis une autre adresse ne survit plus au retrait) ; fermer trop vaut mieux que fermer trop peu sur un poste volé, et le titulaire se reconnecte par mot de passe. Les sessions liées à un AUTRE poste restent. Le poste d'où part la requête ne se retire pas depuis lui-même (`422`). Un poste d'un autre compte, ou qui n'existe pas : `404` (indiscernables).
- Journal : `device.remove`, réussi, avec le nom du poste retiré (nettoyé).
- Un poste est oublié 90 jours après sa dernière preuve (purge horaire).

## Application (code)
- `crates/hearth-agent/src/domain/trust/device.rs::{judge_enrollment, RETENTION, cutoff}` ; `hearth-proto/src/api/devices.rs::MAX_DEVICES_PER_ACCOUNT`.
- `crates/hearth-agent/src/application/trust.rs::TrustService::{list, remove}` ; `entrypoint/http/devices.rs::{list, remove}` ; `application/maintenance.rs::MaintenanceService::purge`.
- Client (HRT-23) : `crates/hearth-link/src/manager/device.rs::{devices_list, remove_trusted_device}` (défi `0x04` signé, mot de passe, action suivie : jamais rejouée) ; coquille : `apps/desktop/src-tauri/src/devices/` (commandes typées `list_trusted_devices`, `remove_trusted_device`, `service::interpret`) ; interface : `apps/desktop/src/pages/Security.vue`, `components/organisms/{TrustedDeviceTable,RemoveDeviceDialog}.vue` (liste, « Ce poste » marqué, « Retirer » grisé sur lui et sur un poste sans clé inscrite, 8 sur 8 avec les voies de secours, fenêtre qui demande le mot de passe).

## Vérification
- `tests/device_proof.rs` : `the_ninth_device_is_refused_without_evicting_another_and_the_connection_still_succeeds`, `the_list_shows_only_the_own_devices_and_marks_the_current_one`, `the_current_device_cannot_be_removed_from_itself_but_another_can`, `removing_a_device_closes_its_sessions_forgets_its_address_and_is_journaled`, `a_device_identifier_that_is_not_ours_cannot_be_removed`, `the_purge_forgets_a_device_after_ninety_days_without_proof_and_detaches_its_session`.
- `tests/device_http.rs` : `the_list_shows_the_devices_of_the_caller_without_any_key_material`, `a_device_is_removed_by_its_owner_only_and_never_from_itself`, `removing_a_device_replays_with_its_operation_key_and_leaves_one_journal_entry`, et pour Q16 : `a_stolen_session_alone_removes_nothing_and_tries_no_password`, `a_stolen_session_and_the_password_without_the_key_removes_nothing`, `a_session_without_an_enrolled_device_gets_a_typed_explicit_refusal`, `the_proof_of_another_account_or_for_another_device_or_session_removes_nothing`, `a_failed_removal_does_not_burn_the_proof_and_a_served_one_is_never_replayed`, `a_wrong_password_on_a_removal_counts_like_a_login_failure_and_ends_in_a_wait` ; proto `device_proof::tests::the_device_removal_layout_binds_the_token_and_the_target_device`.
- Voies de secours : `tests/device_proof.rs::{a_password_change_by_an_administrator_forgets_every_device_and_address, closing_the_sessions_forgets_the_devices_and_the_addresses_of_the_account}`, `tests/account_cli.rs::passwd_changes_the_password_and_closes_the_sessions`.
- `tests/http_api.rs::every_modifying_route_leaves_exactly_one_success_entry` (le retrait laisse une seule entrée réussie).
- HRT-24 : `tests/security_followups.rs::removing_a_device_closes_every_session_without_a_link_except_the_current_one` ; `tests/device_http.rs::a_wrong_password_at_the_confirmation_of_a_removal_leaves_one_entry_of_the_removal_and_no_login`.
- Client (HRT-23) : `crates/hearth-link/tests/device_key.rs::{removing_a_trusted_device_needs_the_password_and_the_proof_of_this_pcs_key, removing_a_device_without_a_key_sends_nothing, a_link_cut_during_a_removal_is_unknown_and_never_replayed}` ; `apps/desktop/src-tauri/tests/devices_runtime.rs::removing_a_device_asks_for_the_password_and_proves_the_key_of_this_pc`, `tests/devices_wire.rs` ; interface : `apps/desktop/src/pages/Security.test.ts` (chaque état de la liste, retrait, mot de passe faux, coupure), `apps/desktop/e2e/security.spec.ts`.

## Cas limites
- « Reconnu manuellement sur la liste » (texte de la spec) n'a pas d'autre sens exploitable que « retiré puis réinscrit » : pas d'éviction automatique (conception, section 18, à confirmer).
- Un poste purgé (90 jours) laisse ses sessions ouvertes, détachées du poste.
- **8 postes inscrits et aucun en main** (aucun poste ne peut prouver sa clé) : le client ne peut retirer rien. Voies de secours : changement du mot de passe par un administrateur, `hearth-agent account passwd` et `hearth-agent account revoke` sur le serveur (à dire dans l'aide du client : fait par HRT-23, `devices.limitHelp`).
- Un mot de passe faux à la confirmation du retrait laisse **une** entrée, celle du retrait (`device.remove`, échoué, « mot de passe actuel incorrect »), et pas une connexion refusée de plus ; les compteurs sont ceux de la connexion.

## Règles liées
- BR-TRUST-004, BR-TRUST-024, ADR-0023.

## Historique
- 2026-10-07 : création (HRT-22, session 2026-10-04-hearth-creation, T32).
- 2026-10-07 : HRT-24 (T33) : toutes les sessions sans lien du compte sont fermées au retrait (sauf la courante), cas « 8 postes, aucun en main », entrée de journal du mot de passe faux.
- 2026-10-07 : HRT-23 (T35) : partie client (liste, retrait avec mot de passe, aide des voies de secours).
- 2026-10-07 : HRT-28 (tranche F) : « poste courant » = poste relié à la session à la connexion, jamais réécrit (BR-TRUST-048) ; règle et contrat `0x04` inchangés.
