//! Le jugement d'un lot de jars déposés (HRT-257, BR-MOD-021, BR-MOD-022, BR-MOD-023) : domaine pur, aucune E/S.
//!
//! L'appelant a déjà lu chaque jar (archive hostile jugée entière, fiches d'identité lues) : ce module décide, fichier
//! par fichier, s'il s'ajoute, s'il remplace un mod installé, ou pourquoi il est refusé. Le client n'est jamais cru :
//! l'agent rejuge tout à l'acceptation de l'opération (HRT-258). Le nom de pose est celui que Hearth choisit
//! (`listing::placed_name`), jamais le nom envoyé.

use super::compat::{self, Candidate, ServerProfile, Verdict};
use super::jar::{JarIdentity, JarLoader};
use super::listing;

/// Au plus ce nombre de dépôts par contrôle (BR-MOD-022).
pub const MAX_BATCH: usize = 8;

/// Ce que la lecture d'un dépôt a donné.
#[derive(Debug, Clone)]
pub struct Deposit {
    /// L'archive est refusée par le lecteur d'archives hostiles (ou illisible).
    pub archive_refused: bool,
    /// Le fichier dépasse la borne d'un jar.
    pub too_large: bool,
    /// Les identités lues (vide : « Non reconnu »).
    pub identities: Vec<JarIdentity>,
}

/// Un fichier déjà installé dans le dossier des mods (jamais « Mods retirés »).
#[derive(Debug, Clone)]
pub struct Installed {
    /// Son nom réel, `.jar` ou `.jar.disabled`.
    pub file: String,
    pub enabled: bool,
    pub identities: Vec<JarIdentity>,
    /// Le pack Modrinth le liste : il reviendrait au démarrage suivant (BR-MOD-024).
    pub from_modrinth_pack: bool,
}

/// Les codes de refus, une liste fermée (jamais une valeur lue dans le jar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Unrecognized,
    ArchiveRefused,
    TooLarge,
    IncompatibleLoader {
        jar: JarLoader,
        server: Option<JarLoader>,
    },
    IncompatibleMinecraft,
    DuplicateInBatch,
    NameTaken,
    InModrinthPack,
    SameVersion,
}

impl Refusal {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Unrecognized => "MOD_UNRECOGNIZED",
            Self::ArchiveRefused => "MOD_ARCHIVE_REFUSED",
            Self::TooLarge => "MOD_TOO_LARGE",
            Self::IncompatibleLoader { .. } => "MOD_INCOMPATIBLE_LOADER",
            Self::IncompatibleMinecraft => "MOD_INCOMPATIBLE_MINECRAFT",
            Self::DuplicateInBatch => "MOD_DUPLICATE_IN_BATCH",
            Self::NameTaken => "MOD_NAME_TAKEN",
            Self::InModrinthPack => "MOD_IN_MODRINTH_PACK",
            Self::SameVersion => "MOD_SAME_VERSION",
        }
    }
}

/// Un dépôt accepté.
#[derive(Debug, Clone)]
pub struct Accepted {
    pub identity: JarIdentity,
    /// Le nom sous lequel Hearth le pose.
    pub target_file: String,
    /// Le mod installé qu'il remplace : (nom du fichier, version).
    pub replaces: Option<(String, Option<String>)>,
    pub verdict: Verdict,
}

pub type Judgement = Result<Accepted, (Refusal, Option<JarIdentity>)>;

fn chosen<'a>(identities: &'a [JarIdentity], profile: &ServerProfile) -> Option<&'a JarIdentity> {
    match profile.loader {
        Some(server) => compat::choose(identities, server),
        None => identities.first(),
    }
}

#[allow(clippy::result_large_err)] // le refus porte l'identité lue : jugé une fois par dépôt, 8 au plus
/// Juge le lot. Un jugement par dépôt, dans l'ordre. `profile.loader` absent : tout est refusé (moteur sans mods).
pub fn judge_deposits(
    profile: &ServerProfile,
    installed: &[Installed],
    batch: &[Deposit],
) -> Vec<Judgement> {
    // 1. Ce que chaque dépôt dit de lui-même, sans regarder les autres.
    let mut results: Vec<Option<Judgement>> = batch
        .iter()
        .map(|deposit| {
            if deposit.archive_refused {
                return Some(Err((Refusal::ArchiveRefused, None)));
            }
            if deposit.too_large {
                return Some(Err((Refusal::TooLarge, None)));
            }
            if deposit.identities.is_empty() {
                return Some(Err((Refusal::Unrecognized, None)));
            }
            let Some(identity) = chosen(&deposit.identities, profile) else {
                return Some(Err((
                    Refusal::IncompatibleLoader {
                        jar: deposit.identities[0].loader,
                        server: profile.loader,
                    },
                    deposit.identities.first().cloned(),
                )));
            };
            if identity.minecraft_declared
                && let Some(req) = &identity.minecraft
                && req.contains(profile.minecraft.as_ref()) == super::version_req::Containment::No
            {
                return Some(Err((
                    Refusal::IncompatibleMinecraft,
                    Some(identity.clone()),
                )));
            }
            None
        })
        .collect();

    // 2. Deux dépôts du même identifiant : les deux sont refusés.
    let ids: Vec<Option<&str>> = batch
        .iter()
        .zip(&results)
        .map(|(deposit, result)| {
            result
                .is_none()
                .then(|| chosen(&deposit.identities, profile).map(|i| i.id.as_str()))
                .flatten()
        })
        .collect();
    for (i, id) in ids.iter().enumerate() {
        let Some(id) = id else { continue };
        if ids
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && *other == Some(*id))
        {
            let identity = chosen(&batch[i].identities, profile).cloned();
            results[i] = Some(Err((Refusal::DuplicateInBatch, identity)));
        }
    }

    // 3. Contre les mods installés : remplacement, même version, pack Modrinth, nom pris.
    let installed_ids: Vec<Option<&JarIdentity>> = installed
        .iter()
        .map(|file| chosen(&file.identities, profile))
        .collect();
    let mut replaced_files: Vec<usize> = Vec::new();
    for (i, deposit) in batch.iter().enumerate() {
        if results[i].is_some() {
            continue;
        }
        let Some(identity) = chosen(&deposit.identities, profile) else {
            continue;
        };
        let same: Vec<usize> = installed_ids
            .iter()
            .enumerate()
            .filter(|(_, other)| other.is_some_and(|o| o.id == identity.id))
            .map(|(at, _)| at)
            .collect();
        let target = listing::placed_name(
            identity.id.as_str(),
            identity.version.as_ref().map(|v| v.as_str()),
        );
        let refusal = if same.iter().any(|&at| installed[at].from_modrinth_pack) {
            Some(Refusal::InModrinthPack)
        } else if same.iter().any(|&at| {
            identity.version.is_some()
                && installed_ids[at].is_some_and(|o| o.version == identity.version)
        }) {
            Some(Refusal::SameVersion)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            results[i] = Some(Err((refusal, Some(identity.clone()))));
            continue;
        }
        // Le fichier remplacé : le mod activé d'abord, sinon un désactivé.
        let replaced = same
            .iter()
            .copied()
            .find(|&at| installed[at].enabled)
            .or_else(|| same.first().copied());
        let taken = installed.iter().enumerate().any(|(at, file)| {
            Some(at) != replaced
                && (file.file.eq_ignore_ascii_case(&target)
                    || file
                        .file
                        .eq_ignore_ascii_case(&listing::disabled_name(&target)))
        });
        if taken {
            results[i] = Some(Err((Refusal::NameTaken, Some(identity.clone()))));
            continue;
        }
        if let Some(at) = replaced {
            replaced_files.push(at);
        }
        results[i] = Some(Ok(Accepted {
            identity: identity.clone(),
            target_file: target,
            replaces: replaced.map(|at| {
                (
                    installed[at].file.clone(),
                    installed_ids[at]
                        .and_then(|o| o.version.as_ref())
                        .map(|v| v.as_str().to_owned()),
                )
            }),
            verdict: Verdict::Unverified,
        }));
    }

    // 4. Le verdict des acceptés, avec ce qui restera : les mods activés non remplacés et les acceptés.
    let accepted: Vec<usize> = results
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Some(Ok(_))))
        .map(|(i, _)| i)
        .collect();
    let mut candidates: Vec<Candidate<'_>> = installed
        .iter()
        .enumerate()
        .filter(|(at, file)| file.enabled && !replaced_files.contains(at))
        .map(|(_, file)| Candidate {
            identities: &file.identities,
            enabled: true,
        })
        .collect();
    let first_new = candidates.len();
    candidates.extend(accepted.iter().map(|&i| Candidate {
        identities: &batch[i].identities,
        enabled: true,
    }));
    let verdicts = compat::judge_batch(profile, &candidates);
    for (k, &i) in accepted.iter().enumerate() {
        if let Some(Ok(done)) = &mut results[i] {
            done.verdict = verdicts[first_new + k].clone();
        }
    }
    results
        .into_iter()
        .map(|result| result.unwrap_or(Err((Refusal::Unrecognized, None))))
        .collect()
}

#[cfg(test)]
mod tests;
