//! Commandes des transferts exposées à l'interface (liste blanche : `build.rs` et `capabilities/default.json`). UNE
//! commande par geste (ADR-0016), aucun paramètre libre : ni route, ni méthode. Ouvrir un dépôt est un acte
//! d'administration confirmé (`FileWrite`, couvert par l'élévation) ; les autres gestes ne touchent que le carnet de
//! cette session et, pour annuler, le dépôt de ce compte sur le serveur.

use std::sync::Arc;

use hearth_link::domain::secret::Secret;
use hearth_link::domain::server::ServerId;
use tauri::State;

use super::dto::{DepositOutcome, DepositSourceDto, TransferDto};
use super::service::{self, TransferBook};
use crate::link::LinkRuntime;
use crate::link_dto::LinkFailure;

type Runtime_<'a> = State<'a, Arc<LinkRuntime>>;
type Book<'a> = State<'a, Arc<TransferBook>>;

/// La boîte « Ouvrir » du système, rattachée à la fenêtre : le chemin du fichier choisi, ou rien si le compte annule
/// (annuler la boîte ne produit aucun message). Le contenu du fichier ne passe jamais par la page.
#[tauri::command]
#[specta::specta]
pub async fn pick_deposit_file(
    window: tauri::WebviewWindow,
) -> Result<Option<DepositSourceDto>, LinkFailure> {
    Ok(rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title(crate::texts::DEPOSIT_DIALOG_TITLE)
        .pick_file()
        .await
        .map(|handle| DepositSourceDto {
            path: handle.path().to_string_lossy().into_owned(),
        }))
}

/// Ouvre un dépôt dans la zone d'attente de l'agent (`POST /deposits`, acte `FileWrite`) puis envoie le fichier en
/// tâche de fond. `admin_password` : le mot de passe de confirmation selon l'état de l'élévation. Jamais rejoué : un
/// lien tombé avant la réponse donne `Unknown`. Sans le droit « gérer les services » : `MissingServicesRight`.
#[tauri::command]
#[specta::specta]
pub async fn start_deposit(
    link: Runtime_<'_>,
    book: Book<'_>,
    server_id: String,
    source: DepositSourceDto,
    admin_password: Option<String>,
) -> Result<DepositOutcome, LinkFailure> {
    let id = ServerId::parse(&server_id).map_err(|_| LinkFailure::UnknownServer)?;
    let admin_password = admin_password.map(Secret::new);
    service::start(
        link.manager(),
        &book,
        &id,
        &source.path,
        admin_password.as_ref(),
    )
    .await
}

/// Les envois de cette session, sans lecture réseau : le panneau « Transferts » les relit tant que l'un envoie.
#[tauri::command]
#[specta::specta]
pub async fn list_transfers(book: Book<'_>) -> Result<Vec<TransferDto>, LinkFailure> {
    Ok(book.list())
}

/// Reprend un envoi interrompu : un geste du compte, jamais automatique.
#[tauri::command]
#[specta::specta]
pub async fn resume_transfer(
    link: Runtime_<'_>,
    book: Book<'_>,
    transfer_id: String,
) -> Result<(), LinkFailure> {
    service::resume(link.manager(), &book, &transfer_id);
    Ok(())
}

/// Annule un envoi : ferme le socket puis retire la partie reçue du serveur.
#[tauri::command]
#[specta::specta]
pub async fn cancel_transfer(
    link: Runtime_<'_>,
    book: Book<'_>,
    transfer_id: String,
) -> Result<(), LinkFailure> {
    service::cancel(link.manager(), &book, &transfer_id);
    Ok(())
}

/// Retire de la liste un envoi fini (réussi, annulé ou échoué). Un envoi encore ouvert reste.
#[tauri::command]
#[specta::specta]
pub async fn dismiss_transfer(book: Book<'_>, transfer_id: String) -> Result<(), LinkFailure> {
    book.dismiss(&transfer_id);
    Ok(())
}
