//! Sous-commandes `install` et `uninstall` : l'échange avec l'utilisateur (questions, messages de
//! la spécification fonctionnelle) autour de `application::install::Installer`.
//!
//! Interactif (questions au terminal, mot de passe sans écho avec confirmation) **ou** sans aucune
//! question (variables `HEARTH_ADMIN_USER`, `HEARTH_ADMIN_PASSWORD` ou
//! `HEARTH_ADMIN_PASSWORD_HASH`, `HEARTH_PORT`, et `--yes`). Tout est contrôlé avant la première
//! écriture. Les mots de passe ne sont jamais imprimés ni passés en argument.

use std::io::{self, Write};
use std::net::IpAddr;
use std::path::PathBuf;

use hearth_proto::product::DEFAULT_PORT;
use thiserror::Error;

use super::cli::{InstallArgs, UninstallArgs};
use crate::application::install::{Event, InstallError, InstallInputs, Installed, Installer};
use crate::application::ports::{AdminCredential, HostError, ServiceKind};
use crate::domain::accounts::Username;
use crate::domain::install::{
    Blocker, DataChoice, InstallKind, InstallPlan, NAME_HELP, Observed, PASSWORD_HELP, Version,
    check_admin_password, check_password_hash_format, check_rights, parse_admin_name, parse_choice,
    parse_port, uninstall_plan,
};
use crate::domain::secret::Secret;

pub const ENV_ADMIN_USER: &str = "HEARTH_ADMIN_USER";
pub const ENV_ADMIN_PASSWORD: &str = "HEARTH_ADMIN_PASSWORD";
pub const ENV_ADMIN_PASSWORD_HASH: &str = "HEARTH_ADMIN_PASSWORD_HASH";
pub const ENV_PORT: &str = "HEARTH_PORT";
pub const ENV_MANAGED: &str = "HEARTH_MANAGED";

// Messages de la spécification fonctionnelle (us-installer-agent, section 4).
pub const MSG_TITLE_INSTALL: &str = "Installation de l'agent Hearth";
pub const MSG_TITLE_UPDATE: &str = "Mise à jour de l'agent Hearth";
pub const MSG_TITLE_UNINSTALL: &str = "Désinstallation de l'agent Hearth";
pub const MSG_CHECK_ARCH: &str = "Vérification de l'architecture système...";
pub const MSG_CHECK_PORT: &str = "Vérification de la disponibilité du port...";
pub const MSG_INSTALLING: &str = "Installation en cours...";
pub const MSG_STARTING: &str = "Démarrage du service...";
pub const MSG_PROMPT_NAME: &str = "Nom du compte administrateur:";
pub const MSG_PROMPT_PASSWORD: &str = "Mot de passe:";
pub const MSG_DETECTED: &str = "Agent détecté. Vérification de la version...";
pub const MSG_FRESH_DONE: &str =
    "Installation réussie. L'agent démarre automatiquement avec ton serveur.";
pub const MSG_UP_TO_DATE_RESTARTED: &str = "L'agent est déjà à jour. Service redémarré.";
pub const MSG_UPDATE_DONE: &str = "Mise à jour réussie. Tes comptes et données sont conservés.";
pub const MSG_FINGERPRINT_NOTE: &str = "Note cette empreinte. Elle apparaîtra dans le client lors de la première connexion. Compare-la avec celle que le client affichera à la première connexion.";
pub const MSG_UNINSTALL_PROMPT: &str =
    "Supprimer aussi les comptes, le journal et la configuration ? (conserver/supprimer)";
pub const MSG_NOT_INSTALLED: &str =
    "L'agent n'est pas installé sur ce serveur. Rien à désinstaller.";
pub const MSG_UNINSTALL_KEPT: &str =
    "Service arrêté. Tes comptes et données sont conservés sur ta machine.";
pub const MSG_UNINSTALL_PURGED: &str = "Service arrêté. Tous les comptes, le journal et la configuration ont été supprimés. Aucune trace de l'agent ne reste sur ta machine.";
pub const MSG_INTERRUPTED: &str =
    "Installation interrompue. Aucune modification n'a été apportée à ta machine.";

// Messages que la spécification ne donne pas (signalés dans le ticket HRT-15).
pub const MSG_CHECK_SPACE: &str = "Vérification de l'espace disque...";
pub const MSG_UP_TO_DATE_UNTOUCHED: &str =
    "L'agent est déjà à jour. Le service n'a pas été interrompu.";
pub const MSG_REPAIR_DONE: &str = "Réparation réussie. Tes comptes et données sont conservés.";
pub const MSG_REPAIRING: &str = "Installation incomplète détectée. Réparation...";
pub const MSG_PASSWORDS_DIFFER: &str = "Les deux mots de passe ne correspondent pas.";
pub const MSG_PROMPT_CONFIRM: &str = "Confirme le mot de passe:";
pub const MSG_PROMPT_PORT: &str = "Numéro de port";
pub const MSG_ALREADY_RUNNING: &str =
    "Une installation est déjà en cours sur ce serveur. Attends qu'elle se termine, puis relance.";
pub const MSG_NOT_WRITTEN: &str = "Installation gérée par le système : aucune unité n'a été écrite et le service n'a pas été lancé. Démarre l'agent avec `hearth-agent serve`, ou laisse ton système le faire.";

#[derive(Debug, Error)]
pub enum InstallCliError {
    /// Un prérequis manque, ou une entrée est refusée : rien n'a été modifié.
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Failed(String),
    #[error("{MSG_INTERRUPTED}")]
    Interrupted,
    #[error("Écriture du résultat impossible : {0}")]
    Output(#[from] io::Error),
}

/// Questions posées à l'utilisateur. L'adaptateur est le terminal (`entrypoint/terminal.rs`).
pub trait Prompter {
    /// Une réponse visible (le libellé est affiché tel quel).
    fn line(&mut self, label: &str) -> io::Result<String>;
    /// Une réponse masquée : aucun caractère n'est affiché.
    fn secret(&mut self, label: &str) -> io::Result<Secret>;
}

/// Ce que l'installation sait de son environnement.
pub struct Context<'a> {
    /// Variables d'environnement (injectables pour les tests).
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// Un terminal répond : les questions sont possibles.
    pub interactive: bool,
    /// Le binaire en cours d'exécution : celui qu'on installe.
    pub source_binary: PathBuf,
    pub listen_addr: IpAddr,
    /// `--data-dir` différent du défaut, à écrire dans la configuration.
    pub data_dir_override: Option<PathBuf>,
    /// Version du binaire qui s'exécute.
    pub target: Version,
    /// La commande telle qu'elle a été tapée, pour proposer `sudo …`.
    pub command_line: String,
    /// systemd n'est pas là et l'installation n'est pas gérée : à refuser.
    pub systemd_missing: bool,
}

/// L'installation est-elle gérée par le système : option `--managed` ou `HEARTH_MANAGED`.
pub fn is_managed(requested: bool, env: &dyn Fn(&str) -> Option<String>) -> bool {
    requested || non_empty(env, ENV_MANAGED).is_some_and(|value| is_true(&value))
}

fn non_empty(env: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    env(name).filter(|value| !value.is_empty())
}

fn is_true(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Le message d'un prérequis manquant, avec la suite à donner (la commande exacte à relancer).
fn refusal(blocker: &Blocker, context: &Context<'_>, port: u16) -> InstallCliError {
    let mut text = blocker.to_string();
    match blocker {
        Blocker::NotPrivileged => {
            text.push_str(&format!("\nExemple : sudo {}", context.command_line));
        }
        Blocker::PortTaken { .. } => {
            let other = if port == u16::MAX { port - 1 } else { port + 1 };
            text.push_str(&format!("\nExemple : hearth-agent install --port {other}"));
        }
        _ => {}
    }
    InstallCliError::Refused(text)
}

pub async fn install(
    args: &InstallArgs,
    context: &Context<'_>,
    installer: &Installer<'_>,
    prompter: &mut dyn Prompter,
    out: &mut dyn Write,
) -> Result<(), InstallCliError> {
    let managed = is_managed(args.managed, context.env);

    // 1. Les droits d'abord : sans eux, on n'observe rien (BR-INSTALL-001).
    if let Err(blocker) = check_rights(installer.host.is_privileged()) {
        writeln!(out, "{MSG_TITLE_INSTALL}")?;
        return Err(refusal(&blocker, context, DEFAULT_PORT));
    }
    // Une seule installation à la fois.
    let _lock = installer
        .host
        .lock(&installer.paths.lock)
        .map_err(|error| match error {
            HostError::AlreadyRunning => InstallCliError::Refused(MSG_ALREADY_RUNNING.to_owned()),
            other => failed(&other.to_string()),
        })?;

    // 2. Ce qu'il y a déjà : ne modifie rien.
    let (observed, facts) = installer
        .observe(&context.source_binary, managed)
        .await
        .map_err(|error| failed(&error.to_string()))?;
    let plan = installer
        .plan(&observed, context.target)
        .map_err(|error| InstallCliError::Refused(error.to_string()))?;
    announce(out, &plan)?;

    // 3. Le port : celui de l'installation existante, sinon celui demandé, sinon 7341.
    let port = choose_port(args, context, &plan, facts.configured_port, prompter, out)?;

    // 4. Les prérequis, avant toute écriture (BR-INSTALL-006, 012).
    writeln!(out, "{MSG_CHECK_ARCH}")?;
    writeln!(out, "{MSG_CHECK_PORT}")?;
    writeln!(out, "{MSG_CHECK_SPACE}")?;
    installer
        .check_prerequisites(&observed, &facts, port, context.listen_addr)
        .map_err(|blocker| refusal(&blocker, context, port))?;
    if context.systemd_missing && !managed {
        return Err(InstallCliError::Refused(
            "systemd est introuvable sur cette machine. Si ton système gère lui-même le service (NixOS par exemple), relance avec --managed : l'agent n'écrira alors aucune unité.".to_owned(),
        ));
    }

    // 5. Le premier compte, si aucun administrateur n'existe (BR-INSTALL-002).
    let admin = if plan.needs_first_admin {
        Some(first_admin(args, context, installer, prompter, out)?)
    } else {
        None
    };

    // 6. Les écritures, avec retour en arrière.
    let inputs = InstallInputs {
        port,
        listen_addr: context.listen_addr,
        admin,
        target: context.target,
        source_binary: context.source_binary.clone(),
        managed,
        data_dir_override: context.data_dir_override.clone(),
    };
    let mut say = |event: Event| {
        let line = match event {
            Event::Installing => MSG_INSTALLING,
            Event::StartingService => MSG_STARTING,
            Event::ServiceNotWritten => MSG_NOT_WRITTEN,
        };
        // Un terminal qui se ferme ne doit pas faire échouer l'installation.
        let _ = writeln!(out, "{line}");
    };
    let installed = installer
        .apply(&plan, &observed, inputs, &mut say)
        .await
        .map_err(|failure| {
            if matches!(failure.cause, InstallError::Interrupted) && failure.left_behind.is_empty()
            {
                return InstallCliError::Interrupted;
            }
            let mut text = format!(
                "Une erreur s'est produite : {}.",
                failure.cause.to_string().trim_end_matches('.')
            );
            if failure.left_behind.is_empty() {
                text.push_str(" Aucune modification n'a été apportée à ta machine.");
            } else {
                text.push_str(&format!(
                    " Le retour en arrière n'a pas pu tout défaire ({}) : nettoie ces éléments à la main.",
                    failure.left_behind.join(" ; ")
                ));
            }
            InstallCliError::Failed(text)
        })?;

    report(out, &installed, installer.host.hostname().as_str())?;
    Ok(())
}

fn failed(detail: &str) -> InstallCliError {
    InstallCliError::Failed(format!(
        "Une erreur s'est produite : {}. Aucune modification n'a été apportée à ta machine.",
        detail.trim_end_matches('.')
    ))
}

/// Le titre et l'annonce selon ce qui est détecté.
fn announce(out: &mut dyn Write, plan: &InstallPlan) -> io::Result<()> {
    match plan.kind {
        InstallKind::Fresh => writeln!(out, "{MSG_TITLE_INSTALL}"),
        InstallKind::Reinstall => {
            writeln!(out, "{MSG_TITLE_UPDATE}")?;
            writeln!(out, "{MSG_DETECTED}")
        }
        InstallKind::Upgrade { from } => {
            writeln!(out, "{MSG_TITLE_UPDATE}")?;
            writeln!(out, "{MSG_DETECTED}")?;
            writeln!(out, "Mise à jour depuis la version {from}...")
        }
        InstallKind::Repair => {
            writeln!(out, "{MSG_TITLE_INSTALL}")?;
            writeln!(out, "{MSG_REPAIRING}")
        }
    }
}

fn choose_port(
    args: &InstallArgs,
    context: &Context<'_>,
    plan: &InstallPlan,
    configured: Option<u16>,
    prompter: &mut dyn Prompter,
    out: &mut dyn Write,
) -> Result<u16, InstallCliError> {
    let asked = args
        .port
        .clone()
        .or_else(|| non_empty(context.env, ENV_PORT));
    // Une installation existante garde son port : en changer exige une reconfiguration
    // manuelle (hors de l'installation automatique).
    if let (Some(current), true) = (configured, plan.kind != InstallKind::Fresh) {
        if let Some(raw) = &asked
            && parse_port(raw).is_ok_and(|requested| requested != current)
        {
            writeln!(
                out,
                "Le port de l'installation existante ({current}) est conservé. Pour en changer, modifie le fichier de configuration puis redémarre le service."
            )?;
        }
        return Ok(current);
    }
    if let Some(raw) = asked {
        return parse_port(&raw).map_err(|error| InstallCliError::Refused(error.to_string()));
    }
    if !context.interactive || args.yes {
        return Ok(DEFAULT_PORT);
    }
    loop {
        let answer = prompter.line(&format!("{MSG_PROMPT_PORT} [{DEFAULT_PORT}]:"))?;
        match parse_port(&answer) {
            Ok(port) => return Ok(port),
            Err(error) => writeln!(out, "{error}")?,
        }
    }
}

/// Le premier compte : par les variables d'environnement, sinon par des questions.
fn first_admin(
    args: &InstallArgs,
    context: &Context<'_>,
    installer: &Installer<'_>,
    prompter: &mut dyn Prompter,
    out: &mut dyn Write,
) -> Result<(Username, AdminCredential), InstallCliError> {
    let can_ask = context.interactive && !args.yes;
    let missing = |what: &str| {
        InstallCliError::Refused(format!(
            "Le premier compte administrateur est requis : {what}. Renseigne {ENV_ADMIN_USER} et {ENV_ADMIN_PASSWORD} (ou {ENV_ADMIN_PASSWORD_HASH}), ou lance la commande dans un terminal."
        ))
    };

    // L'identifiant.
    let name = match non_empty(context.env, ENV_ADMIN_USER) {
        Some(raw) => {
            parse_admin_name(&raw).map_err(|error| InstallCliError::Refused(error.to_string()))?
        }
        None if can_ask => loop {
            writeln!(out, "{NAME_HELP}")?;
            let answer = prompter.line(MSG_PROMPT_NAME)?;
            match parse_admin_name(answer.trim()) {
                Ok(name) => break name,
                Err(error) => writeln!(out, "{error}")?,
            }
        },
        None => return Err(missing(&format!("{ENV_ADMIN_USER} n'est pas défini"))),
    };

    // Le mot de passe : un haché tout fait, un mot de passe en variable, ou la saisie.
    if let Some(hash) = non_empty(context.env, ENV_ADMIN_PASSWORD_HASH) {
        check_password_hash_format(&hash)
            .map_err(|error| InstallCliError::Refused(error.to_string()))?;
        let hash = Secret::new(hash);
        installer.admins.check_hash(&hash).map_err(|_| {
            InstallCliError::Refused(
                crate::domain::install::HashFormatError::NotArgon2id.to_string(),
            )
        })?;
        return Ok((name, AdminCredential::Hash(hash)));
    }
    if let Some(password) = non_empty(context.env, ENV_ADMIN_PASSWORD) {
        check_admin_password(&password, &name)
            .map_err(|error| InstallCliError::Refused(format!("{error}\n{PASSWORD_HELP}")))?;
        return Ok((name, AdminCredential::Password(Secret::new(password))));
    }
    if !can_ask {
        return Err(missing(&format!("{ENV_ADMIN_PASSWORD} n'est pas défini")));
    }
    loop {
        writeln!(out, "{PASSWORD_HELP}")?;
        let first = prompter.secret(MSG_PROMPT_PASSWORD)?;
        if let Err(error) = check_admin_password(first.expose(), &name) {
            writeln!(out, "{error}")?;
            continue;
        }
        let second = prompter.secret(MSG_PROMPT_CONFIRM)?;
        if first.expose() != second.expose() {
            writeln!(out, "{MSG_PASSWORDS_DIFFER}")?;
            continue;
        }
        return Ok((name, AdminCredential::Password(first)));
    }
}

/// Le récapitulatif : succès, empreinte, adresse à saisir dans le client.
fn report(out: &mut dyn Write, installed: &Installed, hostname: &str) -> io::Result<()> {
    if installed.service == ServiceKind::Unmanaged {
        // L'installation gérée ne lance rien : `MSG_NOT_WRITTEN` a déjà été dit.
    } else {
        match installed.kind {
            InstallKind::Fresh => writeln!(out, "{MSG_FRESH_DONE}")?,
            InstallKind::Reinstall => {
                if installed.service_untouched {
                    writeln!(out, "{MSG_UP_TO_DATE_UNTOUCHED}")?;
                } else {
                    writeln!(out, "{MSG_UP_TO_DATE_RESTARTED}")?;
                }
                writeln!(out, "{MSG_UPDATE_DONE}")?;
            }
            InstallKind::Upgrade { .. } => writeln!(out, "{MSG_UPDATE_DONE}")?,
            InstallKind::Repair => writeln!(out, "{MSG_REPAIR_DONE}")?,
        }
    }
    writeln!(out)?;
    writeln!(out, "Empreinte du serveur : {}", installed.fingerprint)?;
    if installed.kind == InstallKind::Fresh {
        writeln!(out, "{MSG_FINGERPRINT_NOTE}")?;
    }
    writeln!(
        out,
        "Adresse à saisir dans le client : {hostname}:{}",
        installed.port
    )?;
    Ok(())
}

pub async fn uninstall(
    args: &UninstallArgs,
    context: &Context<'_>,
    installer: &Installer<'_>,
    prompter: &mut dyn Prompter,
    out: &mut dyn Write,
) -> Result<(), InstallCliError> {
    writeln!(out, "{MSG_TITLE_UNINSTALL}")?;
    if let Err(blocker) = check_rights(installer.host.is_privileged()) {
        return Err(refusal(&blocker, context, DEFAULT_PORT));
    }
    let _lock = installer
        .host
        .lock(&installer.paths.lock)
        .map_err(|error| match error {
            HostError::AlreadyRunning => InstallCliError::Refused(MSG_ALREADY_RUNNING.to_owned()),
            other => failed(&other.to_string()),
        })?;
    let managed = is_managed(args.managed, context.env);
    let (observed, _facts) = installer
        .observe(&context.source_binary, managed)
        .await
        .map_err(|error| failed(&error.to_string()))?;

    if uninstall_plan(&observed, DataChoice::Keep).nothing_to_do {
        writeln!(out, "{MSG_NOT_INSTALLED}")?;
        return Ok(());
    }
    let choice = choose_data(args, context, &observed, prompter, out)?;
    let plan = uninstall_plan(&observed, choice);
    let result = installer.uninstall(&plan);
    if !result.failed.is_empty() {
        return Err(InstallCliError::Failed(format!(
            "La désinstallation n'est pas complète : {}. Nettoie ces éléments à la main.",
            result.failed.join(" ; ")
        )));
    }
    match choice {
        DataChoice::Keep => writeln!(out, "{MSG_UNINSTALL_KEPT}")?,
        DataChoice::Purge if result.foreign.is_empty() => writeln!(out, "{MSG_UNINSTALL_PURGED}")?,
        DataChoice::Purge => writeln!(
            out,
            "Service arrêté. Les comptes, le journal et la configuration ont été supprimés. Le dossier de données contient aussi des fichiers qui ne sont pas à Hearth ; ils n'ont pas été touchés : {}.",
            result.foreign.join(", ")
        )?,
    }
    Ok(())
}

fn choose_data(
    args: &UninstallArgs,
    context: &Context<'_>,
    observed: &Observed,
    prompter: &mut dyn Prompter,
    out: &mut dyn Write,
) -> Result<DataChoice, InstallCliError> {
    if args.purge {
        return Ok(DataChoice::Purge);
    }
    if args.keep_data || args.yes {
        return Ok(DataChoice::Keep);
    }
    // Rien à décider si aucune donnée ne reste.
    if !observed.data.dir_exists && !observed.data.any() && !observed.config_exists {
        return Ok(DataChoice::Keep);
    }
    if !context.interactive {
        return Err(InstallCliError::Refused(
            "Sans terminal, précise --keep-data ou --purge (ou --yes pour tout conserver)."
                .to_owned(),
        ));
    }
    loop {
        let answer = prompter.line(MSG_UNINSTALL_PROMPT)?;
        match parse_choice(&answer) {
            Ok(choice) => return Ok(choice),
            Err(error) => writeln!(out, "{error}")?,
        }
    }
}
