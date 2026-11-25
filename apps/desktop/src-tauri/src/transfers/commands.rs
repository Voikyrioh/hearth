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

/// La boîte « Ouvrir » du système avec plusieurs fichiers (les mods d'un lot, HRT-262) : les chemins choisis, ou rien si
/// le compte annule la boîte. Le contenu des fichiers ne passe jamais par la page.
#[tauri::command]
#[specta::specta]
pub async fn pick_deposit_files(
    window: tauri::WebviewWindow,
) -> Result<Vec<DepositSourceDto>, LinkFailure> {
    Ok(rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title(crate::texts::DEPOSIT_DIALOG_TITLE)
        .pick_files()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|handle| DepositSourceDto {
            path: handle.path().to_string_lossy().into_owned(),
        })
        .collect())
}

/// L'empreinte SHA-256 (hexadécimal, minuscules) d'un fichier local choisi par la boîte « Ouvrir » : l'agent la compare à
/// celle qu'il lit dans la zone d'attente avant de poser un mod (BR-MOD-022). Lue par morceaux sur un fil bloquant ; un
/// fichier illisible rend `None` (jamais une empreinte devinée).
#[tauri::command]
#[specta::specta]
pub async fn file_sha256(source: DepositSourceDto) -> Result<Option<String>, LinkFailure> {
    Ok(tokio::task::spawn_blocking(move || sha256_of(&source.path))
        .await
        .unwrap_or(None))
}

/// SHA-256 d'un fichier, lu par morceaux de 1 Mio.
pub fn sha256_of(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
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

/// Les envois de cette session : le panneau « Transferts » les relit tant que l'un envoie. La réponse n'attend
/// jamais le réseau ; en tâche de fond, les envois réussis dont le fichier a quitté le serveur (consommé par une
/// installation, ou expiré) sortent de la liste (HRT-222).
#[tauri::command]
#[specta::specta]
pub async fn list_transfers(
    link: Runtime_<'_>,
    book: Book<'_>,
) -> Result<Vec<TransferDto>, LinkFailure> {
    let manager = link.manager().clone();
    let tracked = Arc::clone(book.inner());
    tauri::async_runtime::spawn(async move { service::reconcile(&manager, &tracked).await });
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

#[cfg(test)]
mod sha_tests {
    use super::sha256_of;

    #[test]
    fn the_digest_is_the_sha256_of_the_whole_file_and_an_unreadable_file_has_none() {
        let dir = std::env::temp_dir().join(format!("hearth-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("abc.bin");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256_of(file.to_str().unwrap()).as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert!(sha256_of(dir.join("absent").to_str().unwrap()).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
