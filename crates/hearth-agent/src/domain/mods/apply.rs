//! Le plan d'un lot de mods à appliquer (HRT-258, BR-MOD-024 à 026, 032) : domaine pur, aucune E/S.
//!
//! L'appelant a lu le dossier des mods et rejugé les dépôts ; ce module valide les bornes du lot, refuse ce qui ne
//! peut pas s'appliquer, puis ordonne les gestes : retraits (actifs et désactivés), désactivations, remplacements,
//! réactivations, remises, ajouts (ce qui libère un identifiant passe avant ce qui l'occupe). Rien n'est jamais
//! supprimé : tout geste est une pose ou un renommage sans remplacement.

use std::collections::HashSet;

use hearth_proto::api::mods::ModsApplyRequest;

use super::compat::{self, ServerProfile};
use super::deposit::{Installed, Judgement};
use super::listing;
use crate::domain::ops::MoveKind;

/// Au plus ce nombre de changements par lot (dont 8 ajouts).
pub const MAX_CHANGES: usize = 64;
pub const MAX_ADDS: usize = 8;

/// Pourquoi un lot est refusé, avant tout geste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyRefusal {
    /// Bornes, noms, doublons (`MODS_BATCH_INVALID`).
    Batch,
    /// Un dépôt rejugé est refusé, ou `replace` ne désigne pas le mod de même identifiant (`MOD_REFUSED`).
    Refused { deposit: String, refusal: String },
    /// Un nom absent du dossier visé, ou pas dans le bon état (`MOD_FILE_MISSING`).
    FileMissing,
    /// Retrait, remplacement ou désactivation d'un mod du pack Modrinth (`MOD_IN_MODRINTH_PACK`).
    InModrinthPack,
    /// Réactivation alors qu'un autre fichier actif porte le même identifiant (`MOD_ALREADY_ACTIVE`).
    AlreadyActive,
    /// Nom d'arrivée déjà pris (`MOD_NAME_TAKEN`).
    NameTaken,
}

/// Un geste, dans l'ordre. Les noms sont ceux de fichiers : `mods` et `retires` sont les deux dossiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Un renommage sans remplacement : `from` et `to` sont des noms, dans le dossier de leur genre.
    Move {
        kind: MoveKind,
        from: String,
        to: String,
    },
    /// La pose du dépôt numéro `add` (rang dans `request.add`) sous `file` dans `mods`.
    Place { add: usize, file: String },
}

/// Un dépôt rejugé, dans l'ordre de `request.add`.
pub struct JudgedAdd {
    pub deposit: String,
    pub judgement: Judgement,
}

/// Ce que l'appelant a lu avant d'appeler [`plan`].
pub struct Input<'a> {
    pub profile: &'a ServerProfile,
    /// Les fichiers de `mods` (actifs et `.disabled`).
    pub installed: &'a [Installed],
    /// Les noms de `mods-retires`.
    pub retired: &'a [String],
    pub adds: &'a [JudgedAdd],
}

fn has(names: &HashSet<String>, name: &str) -> bool {
    names.iter().any(|n| n.eq_ignore_ascii_case(name))
}

/// Les bornes du lot, sans rien regarder du dossier.
pub fn validate(request: &ModsApplyRequest) -> Result<(), ApplyRefusal> {
    let total = request.total();
    if total == 0 || total > MAX_CHANGES || request.add.len() > MAX_ADDS {
        return Err(ApplyRefusal::Batch);
    }
    let named = request
        .retire
        .iter()
        .chain(&request.restore)
        .chain(&request.disable)
        .chain(&request.enable)
        .chain(request.add.iter().filter_map(|a| a.replace.as_ref()));
    let mut seen_mods = HashSet::new();
    for name in named {
        if !listing::is_safe_file_name(name)
            || !(listing::is_active_jar(name) || listing::is_disabled_jar(name))
        {
            return Err(ApplyRefusal::Batch);
        }
    }
    // Un fichier de `mods` n'est touché qu'une fois : retrait, désactivation, réactivation, remplacement. Un fichier
    // de `mods-retires` n'est remis qu'une fois (les deux dossiers n'ont pas les mêmes noms).
    for name in request
        .retire
        .iter()
        .chain(&request.disable)
        .chain(&request.enable)
        .chain(request.add.iter().filter_map(|a| a.replace.as_ref()))
    {
        if !seen_mods.insert(name.to_ascii_lowercase()) {
            return Err(ApplyRefusal::Batch);
        }
    }
    let mut seen_retired = HashSet::new();
    if !request
        .restore
        .iter()
        .all(|name| seen_retired.insert(name.to_ascii_lowercase()))
    {
        return Err(ApplyRefusal::Batch);
    }
    if !request.disable.iter().all(|n| listing::is_active_jar(n))
        || !request.enable.iter().all(|n| listing::is_disabled_jar(n))
    {
        return Err(ApplyRefusal::Batch);
    }
    let mut deposits = HashSet::new();
    if !request
        .add
        .iter()
        .all(|a| deposits.insert(a.deposit.clone()))
    {
        return Err(ApplyRefusal::Batch);
    }
    Ok(())
}

/// Le plan, ou le refus. `request` a déjà passé [`validate`] ; `input.adds` est parallèle à `request.add`.
pub fn plan(request: &ModsApplyRequest, input: &Input<'_>) -> Result<Vec<Action>, ApplyRefusal> {
    // Les dépôts : refusés, ou `replace` qui ne désigne pas le fichier que le jugement remplace.
    for (add, judged) in request.add.iter().zip(input.adds) {
        match &judged.judgement {
            Err((refusal, _)) => {
                return Err(ApplyRefusal::Refused {
                    deposit: judged.deposit.clone(),
                    refusal: refusal.code().to_owned(),
                });
            }
            Ok(accepted) => {
                let judged_replace = accepted.replaces.as_ref().map(|(file, _)| file.as_str());
                if judged_replace != add.replace.as_deref() {
                    return Err(ApplyRefusal::Refused {
                        deposit: judged.deposit.clone(),
                        refusal: "MOD_REPLACE_MISMATCH".to_owned(),
                    });
                }
            }
        }
    }
    let in_mods = |name: &str| {
        input
            .installed
            .iter()
            .find(|f| f.file.eq_ignore_ascii_case(name))
    };
    // Présence dans le bon état.
    for name in request.retire.iter().chain(&request.disable) {
        let Some(file) = in_mods(name) else {
            return Err(ApplyRefusal::FileMissing);
        };
        if file.from_modrinth_pack {
            return Err(ApplyRefusal::InModrinthPack);
        }
    }
    for name in &request.enable {
        if in_mods(name).is_none() {
            return Err(ApplyRefusal::FileMissing);
        }
    }
    for name in &request.restore {
        if !input.retired.iter().any(|r| r.eq_ignore_ascii_case(name)) {
            return Err(ApplyRefusal::FileMissing);
        }
    }
    for add in &request.add {
        if let Some(name) = &add.replace {
            let Some(file) = in_mods(name) else {
                return Err(ApplyRefusal::FileMissing);
            };
            if file.from_modrinth_pack {
                return Err(ApplyRefusal::InModrinthPack);
            }
        }
    }

    // Les noms présents dans `mods` au fil du plan (les arrivées sont jugées contre ce qui reste).
    let mut mods_names: HashSet<String> = input.installed.iter().map(|f| f.file.clone()).collect();
    let mut retired_names: HashSet<String> = input.retired.iter().cloned().collect();
    let mut actions = Vec::new();

    let retire_one = |name: &str,
                      mods_names: &mut HashSet<String>,
                      retired_names: &mut HashSet<String>,
                      actions: &mut Vec<Action>|
     -> Result<(), ApplyRefusal> {
        let free = (1..=listing::MAX_RETIRED_SUFFIX)
            .filter_map(|n| listing::retired_name(name, n))
            .find(|candidate| !has(retired_names, candidate))
            .ok_or(ApplyRefusal::NameTaken)?;
        mods_names.retain(|n| !n.eq_ignore_ascii_case(name));
        retired_names.insert(free.clone());
        actions.push(Action::Move {
            kind: MoveKind::Retired,
            from: name.to_owned(),
            to: free,
        });
        Ok(())
    };

    // 1. Retraits (actifs et désactivés).
    for name in &request.retire {
        retire_one(name, &mut mods_names, &mut retired_names, &mut actions)?;
    }
    // 2. Désactivations.
    for name in &request.disable {
        let to = listing::disabled_name(name);
        if has(&mods_names, &to) {
            return Err(ApplyRefusal::NameTaken);
        }
        mods_names.retain(|n| !n.eq_ignore_ascii_case(name));
        mods_names.insert(to.clone());
        actions.push(Action::Move {
            kind: MoveKind::Disabled,
            from: name.clone(),
            to,
        });
    }
    // 3. Remplacements : l'ancien part dans « Mods retirés » ; le nouveau est posé avec les ajouts (plus bas), l'ancien
    // fichier étant parti d'abord.
    for add in &request.add {
        if let Some(old) = &add.replace {
            retire_one(old, &mut mods_names, &mut retired_names, &mut actions)?;
        }
    }
    // Les fichiers actifs qui resteront, pour « déjà actif ».
    let mut active_ids: Vec<(String, String)> = Vec::new();
    for file in input.installed {
        let gone = !has(&mods_names, &file.file);
        if gone || !file.enabled {
            continue;
        }
        if let Some(identity) = input
            .profile
            .loader
            .and_then(|l| compat::choose(&file.identities, l))
        {
            active_ids.push((identity.id.as_str().to_owned(), file.file.clone()));
        }
    }
    // 4. Réactivations.
    for name in &request.enable {
        let to = name[..name.len() - ".disabled".len()].to_owned();
        let file = in_mods(name).ok_or(ApplyRefusal::FileMissing)?;
        if let Some(identity) = input
            .profile
            .loader
            .and_then(|l| compat::choose(&file.identities, l))
        {
            let taken_by_active = active_ids.iter().any(|(id, _)| id == identity.id.as_str());
            let taken_by_add = input.adds.iter().any(|a| {
                a.judgement
                    .as_ref()
                    .is_ok_and(|ok| ok.identity.id == identity.id)
            });
            if taken_by_active || taken_by_add {
                return Err(ApplyRefusal::AlreadyActive);
            }
            active_ids.push((identity.id.as_str().to_owned(), to.clone()));
        }
        if has(&mods_names, &to) {
            return Err(ApplyRefusal::NameTaken);
        }
        mods_names.retain(|n| !n.eq_ignore_ascii_case(name));
        mods_names.insert(to.clone());
        actions.push(Action::Move {
            kind: MoveKind::Enabled,
            from: name.clone(),
            to,
        });
    }
    // 5. Remises.
    for name in &request.restore {
        if has(&mods_names, name) {
            return Err(ApplyRefusal::NameTaken);
        }
        retired_names.retain(|n| !n.eq_ignore_ascii_case(name));
        mods_names.insert(name.clone());
        actions.push(Action::Move {
            kind: MoveKind::Restored,
            from: name.clone(),
            to: name.clone(),
        });
    }
    // 6. Ajouts.
    for (rank, judged) in input.adds.iter().enumerate() {
        let Ok(accepted) = &judged.judgement else {
            continue;
        };
        let target = accepted.target_file.clone();
        if has(&mods_names, &target) || has(&mods_names, &listing::disabled_name(&target)) {
            return Err(ApplyRefusal::NameTaken);
        }
        mods_names.insert(target.clone());
        actions.push(Action::Place {
            add: rank,
            file: target,
        });
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth_proto::api::mods::ModAdd;

    fn request() -> ModsApplyRequest {
        ModsApplyRequest::default()
    }

    fn installed(file: &str) -> Installed {
        Installed {
            file: file.to_owned(),
            enabled: !listing::is_disabled_jar(file),
            identities: Vec::new(),
            from_modrinth_pack: false,
        }
    }

    fn run(
        request: &ModsApplyRequest,
        installed: &[Installed],
        retired: &[&str],
    ) -> Result<Vec<Action>, ApplyRefusal> {
        validate(request)?;
        let retired: Vec<String> = retired.iter().map(|s| (*s).to_owned()).collect();
        let profile = ServerProfile {
            loader: None,
            minecraft: None,
        };
        plan(
            request,
            &Input {
                profile: &profile,
                installed,
                retired: &retired,
                adds: &[],
            },
        )
    }

    #[test]
    fn the_batch_bounds_names_and_duplicates_are_checked_before_anything() {
        assert_eq!(validate(&request()), Err(ApplyRefusal::Batch));
        let mut many = request();
        many.retire = (0..65).map(|n| format!("m{n:03}.jar")).collect();
        assert_eq!(validate(&many), Err(ApplyRefusal::Batch));
        let mut twice = request();
        twice.retire = vec!["a.jar".into()];
        twice.disable = vec!["a.jar".into()];
        assert_eq!(validate(&twice), Err(ApplyRefusal::Batch));
        let mut unsafe_name = request();
        unsafe_name.retire = vec!["../a.jar".into()];
        assert_eq!(validate(&unsafe_name), Err(ApplyRefusal::Batch));
        let mut wrong = request();
        wrong.disable = vec!["a.jar.disabled".into()];
        assert_eq!(validate(&wrong), Err(ApplyRefusal::Batch));
        let mut wrong = request();
        wrong.enable = vec!["a.jar".into()];
        assert_eq!(validate(&wrong), Err(ApplyRefusal::Batch));
        let mut nine = request();
        nine.add = (0..9)
            .map(|n| ModAdd {
                deposit: format!("D{n}"),
                sha256: String::new(),
                replace: None,
            })
            .collect();
        assert_eq!(validate(&nine), Err(ApplyRefusal::Batch));
    }

    #[test]
    fn the_gestures_run_in_the_documented_order() {
        let mut r = request();
        r.restore = vec!["back.jar".into()];
        r.enable = vec!["on.jar.disabled".into()];
        r.disable = vec!["off.jar".into()];
        r.retire = vec!["gone.jar".into()];
        let files = [
            installed("gone.jar"),
            installed("off.jar"),
            installed("on.jar.disabled"),
        ];
        let actions = run(&r, &files, &["back.jar"]).expect("plan");
        let kinds: Vec<MoveKind> = actions
            .iter()
            .map(|a| match a {
                Action::Move { kind, .. } => *kind,
                Action::Place { .. } => MoveKind::Placed,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                MoveKind::Retired,
                MoveKind::Disabled,
                MoveKind::Enabled,
                MoveKind::Restored
            ]
        );
    }

    #[test]
    fn a_retired_name_already_used_gets_the_next_free_suffix() {
        let mut r = request();
        r.retire = vec!["a.jar".into()];
        let actions = run(&r, &[installed("a.jar")], &["a.jar", "a-2.jar"]).expect("plan");
        assert_eq!(
            actions,
            vec![Action::Move {
                kind: MoveKind::Retired,
                from: "a.jar".into(),
                to: "a-3.jar".into()
            }]
        );
    }

    #[test]
    fn a_missing_file_a_taken_name_and_the_modrinth_pack_are_refused() {
        let mut r = request();
        r.retire = vec!["nope.jar".into()];
        assert_eq!(run(&r, &[], &[]), Err(ApplyRefusal::FileMissing));
        let mut r = request();
        r.restore = vec!["a.jar".into()];
        assert_eq!(run(&r, &[], &[]), Err(ApplyRefusal::FileMissing));
        let mut r = request();
        r.restore = vec!["a.jar".into()];
        assert_eq!(
            run(&r, &[installed("a.jar")], &["a.jar"]),
            Err(ApplyRefusal::NameTaken)
        );
        let mut r = request();
        r.disable = vec!["a.jar".into()];
        assert_eq!(
            run(&r, &[installed("a.jar"), installed("a.jar.disabled")], &[]),
            Err(ApplyRefusal::NameTaken)
        );
        let mut r = request();
        r.enable = vec!["a.jar.disabled".into()];
        assert_eq!(
            run(&r, &[installed("a.jar"), installed("a.jar.disabled")], &[]),
            Err(ApplyRefusal::NameTaken)
        );
        let mut packed = installed("p.jar");
        packed.from_modrinth_pack = true;
        let mut r = request();
        r.retire = vec!["p.jar".into()];
        assert_eq!(
            run(&r, &[packed.clone()], &[]),
            Err(ApplyRefusal::InModrinthPack)
        );
        let mut r = request();
        r.disable = vec!["p.jar".into()];
        assert_eq!(run(&r, &[packed], &[]), Err(ApplyRefusal::InModrinthPack));
    }
}
