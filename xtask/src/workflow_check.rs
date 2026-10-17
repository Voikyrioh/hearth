//! Contrôles du flux `.github/workflows/publish.yml` (HRT-175, BR-RELEASE-004, 005 et 011), tests seulement.
//!
//! Le flux ne se teste pas en le lançant : un lancement réel signe et dépose. Ces tests lisent le fichier
//! COMME DU TEXTE (aucune dépendance YAML ajoutée) et vérifient des lignes exactes : un changement de forme
//! du fichier les fait échouer, c'est voulu. Chaque contrôle est une fonction qui rend la liste de ses
//! violations ; chaque test vérifie que le vrai fichier n'en a aucune ET que le contrôle en voit une quand
//! on abîme une copie du fichier (un contrôle qui ne voit rien serait un faux sentiment de sécurité).
//!
//! Forme attendue du fichier (la même que `ci.yml`) : sections de tête à 0 espace, jobs à 2, clés d'un job
//! à 4, étapes (`- `) à 6, clés d'une étape à 8, leurs enfants à 10.

use std::path::Path;

const WORKFLOW: &str = ".github/workflows/publish.yml";
const JOBS: [&str; 6] = [
    "preflight",
    "build-client",
    "build-agent",
    "review",
    "publish",
    "release-github",
];
/// Les seuls secrets que le flux nomme, tous dans l'environnement `release`.
const SECRETS: [&str; 6] = [
    "HEARTH_CLIENT_SIGNING_KEY",
    "HEARTH_CLIENT_SIGNING_KEY_PASSWORD",
    "HEARTH_AGENT_SIGNING_KEY",
    "HEARTH_AGENT_SIGNING_KEY_PASSWORD",
    "HEARTH_RELEASES_ACCESS_KEY_ID",
    "HEARTH_RELEASES_SECRET_ACCESS_KEY",
];
const ONLY_REAL: &str = "inputs.essai == false";

#[derive(Clone, Debug)]
struct Line {
    no: usize,
    indent: usize,
    text: String,
}

struct Job {
    name: String,
    body: Vec<Line>,
}

fn read() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join(WORKFLOW))
        .unwrap_or_default();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} illisible : {error}", path.display()))
        .replace("\r\n", "\n")
}

/// Les lignes utiles : ni vides ni commentaires (les commentaires de tête citent des noms de secrets).
fn parse(text: &str) -> Vec<Line> {
    text.lines()
        .enumerate()
        .filter_map(|(index, raw)| {
            let trimmed = raw.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            Some(Line {
                no: index + 1,
                indent: raw.len() - trimmed.len(),
                text: trimmed.trim_end().to_owned(),
            })
        })
        .collect()
}

/// Le contenu d'une section de tête (`on:`, `jobs:`, …).
fn top<'a>(lines: &'a [Line], key: &str) -> Option<&'a [Line]> {
    let start = lines
        .iter()
        .position(|line| line.indent == 0 && line.text.starts_with(key))?;
    let len = lines[start + 1..]
        .iter()
        .take_while(|line| line.indent > 0)
        .count();
    Some(&lines[start + 1..start + 1 + len])
}

fn jobs(lines: &[Line]) -> Vec<Job> {
    let mut found: Vec<Job> = Vec::new();
    for line in top(lines, "jobs:").unwrap_or_default() {
        if line.indent == 2 && line.text.ends_with(':') {
            found.push(Job {
                name: line.text.trim_end_matches(':').to_owned(),
                body: Vec::new(),
            });
        } else if let Some(job) = found.last_mut() {
            job.body.push(line.clone());
        }
    }
    found
}

/// Les lignes d'un job avant `steps:` (clés du job).
fn job_keys(job: &Job) -> &[Line] {
    let end = job
        .body
        .iter()
        .position(|line| line.indent == 4 && line.text == "steps:")
        .unwrap_or(job.body.len());
    &job.body[..end]
}

/// Les étapes d'un job : la première ligne (`- uses: …`) est ramenée au niveau des clés de l'étape.
fn steps(job: &Job) -> Vec<Vec<Line>> {
    let Some(start) = job
        .body
        .iter()
        .position(|line| line.indent == 4 && line.text == "steps:")
    else {
        return Vec::new();
    };
    let mut found: Vec<Vec<Line>> = Vec::new();
    for line in &job.body[start + 1..] {
        if line.indent == 6 && line.text.starts_with("- ") {
            found.push(vec![Line {
                no: line.no,
                indent: 8,
                text: line.text[2..].to_owned(),
            }]);
        } else if let Some(step) = found.last_mut() {
            step.push(line.clone());
        }
    }
    found
}

fn has(lines: &[Line], indent: usize, text: &str) -> bool {
    lines
        .iter()
        .any(|line| line.indent == indent && line.text == text)
}

fn value_of<'a>(lines: &'a [Line], indent: usize, key: &str) -> Option<&'a str> {
    lines
        .iter()
        .find(|line| line.indent == indent && line.text.starts_with(key))
        .map(|line| line.text[key.len()..].trim())
}

fn needs(job: &Job) -> Vec<String> {
    let value = value_of(job_keys(job), 4, "needs:").unwrap_or_default();
    value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Le script d'une étape `run:` : la valeur en ligne puis les lignes plus indentées.
fn run_script(step: &[Line]) -> Option<Vec<String>> {
    let position = step
        .iter()
        .position(|line| line.indent == 8 && line.text.starts_with("run:"))?;
    let mut script = Vec::new();
    let inline = step[position].text["run:".len()..].trim();
    if !inline.is_empty() && !matches!(inline, "|" | ">" | "|-" | ">-") {
        script.push(inline.to_owned());
    }
    for line in &step[position + 1..] {
        if line.indent <= 8 {
            break;
        }
        script.push(line.text.clone());
    }
    Some(script)
}

// --- BR-RELEASE-004 : le déclencheur ------------------------------------------------------------

fn trigger_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    let Some(on) = top(&lines, "on:") else {
        return vec!["section on: absente".to_owned()];
    };
    if lines
        .iter()
        .any(|line| line.indent == 0 && line.text.starts_with("on:") && line.text != "on:")
    {
        found.push("`on:` doit être une table, jamais une valeur en ligne".to_owned());
    }
    for line in on.iter().filter(|line| line.indent == 2) {
        if line.text != "workflow_dispatch:" {
            found.push(format!(
                "ligne {} : déclencheur `{}` interdit, seul workflow_dispatch est permis",
                line.no, line.text
            ));
        }
    }
    if !on
        .iter()
        .any(|line| line.indent == 2 && line.text == "workflow_dispatch:")
    {
        found.push("workflow_dispatch absent".to_owned());
    }
    // Les entrées : `essai` coché par défaut, `notes` obligatoire.
    let inputs_ok = |name: &str, wanted: &[&str]| {
        let Some(start) = on
            .iter()
            .position(|line| line.indent == 6 && line.text == format!("{name}:"))
        else {
            return false;
        };
        let body: Vec<&Line> = on[start + 1..]
            .iter()
            .take_while(|line| line.indent > 6)
            .collect();
        wanted
            .iter()
            .all(|want| body.iter().any(|line| line.text == *want))
    };
    if !inputs_ok("essai", &["type: boolean", "default: true"]) {
        found.push("l'entrée `essai` doit être une case cochée par défaut".to_owned());
    }
    if !inputs_ok("notes", &["required: true", "type: string"]) {
        found.push("l'entrée `notes` doit être un texte obligatoire".to_owned());
    }
    found
}

// --- BR-RELEASE-005 : aucune clé avant l'approbation --------------------------------------------

/// Chaque mention `secrets.` doit être dans l'`env:` (clés à 10) d'une ÉTAPE du job `publish`, et ne nommer
/// qu'un des six secrets de l'environnement `release`.
fn secret_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !line.text.contains("secrets.") {
            continue;
        }
        let parent = lines[..index]
            .iter()
            .rev()
            .find(|before| before.indent < line.indent);
        let in_step_env = line.indent == 10
            && parent.is_some_and(|before| before.indent == 8 && before.text == "env:");
        let job = lines[..index]
            .iter()
            .rev()
            .find(|before| before.indent == 2 && before.text.ends_with(':'));
        let in_publish = job.is_some_and(|before| before.text == "publish:");
        if !(in_step_env && in_publish) {
            found.push(format!(
                "ligne {} : `{}` hors de l'env: d'une étape du job publish",
                line.no, line.text
            ));
        }
        let names_known = SECRETS
            .iter()
            .any(|name| line.text.contains(&format!("secrets.{name} ")));
        if !names_known {
            found.push(format!("ligne {} : secret inconnu ou mal formé", line.no));
        }
    }
    for job in jobs(&lines) {
        let keys = job_keys(&job);
        let environment = value_of(keys, 4, "environment:");
        match (job.name.as_str(), environment) {
            ("publish", Some("release")) => {}
            ("publish", other) => found.push(format!(
                "publish doit déclarer `environment: release` (lu : {other:?})"
            )),
            (name, Some(_)) => found.push(format!("{name} ne doit déclarer aucun environnement")),
            (_, None) => {}
        }
    }
    // Aucune clé avant l'approbation : `publish` passe après `review` et après les deux constructions, et
    // aucun autre job que `publish` n'attend après `review`.
    let all = jobs(&lines);
    if let Some(publish) = all.iter().find(|job| job.name == "publish") {
        let waits = needs(publish);
        for required in ["preflight", "build-client", "build-agent", "review"] {
            if !waits.iter().any(|name| name == required) {
                found.push(format!("publish doit dépendre de {required}"));
            }
        }
    } else {
        found.push("job publish absent".to_owned());
    }
    found
}

fn permission_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    if !has(&lines, 0, "permissions: {}") {
        found.push("permissions: {} absent en tête du fichier".to_owned());
    }
    for job in jobs(&lines) {
        let declared = job_keys(&job)
            .iter()
            .any(|line| line.indent == 4 && line.text.starts_with("permissions:"));
        if !declared {
            found.push(format!(
                "le job {} ne déclare pas ses permissions",
                job.name
            ));
        }
    }
    found
}

fn write_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    for line in &lines {
        if line.text.ends_with(": write") || line.text.contains("write-all") {
            let job = lines
                .iter()
                .rev()
                .skip_while(|before| before.no >= line.no)
                .find(|before| before.indent == 2 && before.text.ends_with(':'));
            let allowed = line.text == "contents: write"
                && job.is_some_and(|before| before.text == "release-github:");
            if !allowed {
                found.push(format!(
                    "ligne {} : droit d'écriture `{}` hors de release-github",
                    line.no, line.text
                ));
            }
        }
    }
    let all = jobs(&lines);
    let writes = all
        .iter()
        .any(|job| job.name == "release-github" && has(job_keys(job), 6, "contents: write"));
    if !writes {
        found.push("release-github doit avoir contents: write".to_owned());
    }
    found
}

fn checkout_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    let mut count = 0;
    for job in jobs(&lines) {
        for step in steps(&job) {
            if !step
                .iter()
                .any(|line| line.text.starts_with("uses: actions/checkout@"))
            {
                continue;
            }
            count += 1;
            if !has(&step, 10, "persist-credentials: false") {
                found.push(format!(
                    "{} : un checkout sans persist-credentials: false (ligne {})",
                    job.name, step[0].no
                ));
            }
            if !has(&step, 10, "ref: ${{ github.sha }}") {
                found.push(format!(
                    "{} : un checkout qui n'extrait pas github.sha (ligne {})",
                    job.name, step[0].no
                ));
            }
        }
    }
    // `publish` et `release-github` doivent en avoir un (les quatre autres aussi : `review` n'en a pas).
    if count != 5 {
        found.push(format!("{count} checkout(s) trouvés, 5 attendus"));
    }
    found
}

fn input_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    for job in jobs(&lines) {
        for step in steps(&job) {
            for line in run_script(&step).unwrap_or_default() {
                if line.contains("${{") {
                    found.push(format!(
                        "{} : `${{{{ … }}}}` dans un run: ({line}) ; passer par env:",
                        job.name
                    ));
                }
            }
        }
    }
    found
}

fn pin_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    for line in &lines {
        let Some(used) = line
            .text
            .strip_prefix("- uses: ")
            .or_else(|| line.text.strip_prefix("uses: "))
        else {
            continue;
        };
        let reference = used.split_whitespace().next().unwrap_or_default();
        let sha = reference.split_once('@').map(|(_, sha)| sha);
        let pinned = sha.is_some_and(|sha| {
            sha.len() == 40 && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        });
        if !pinned {
            found.push(format!(
                "ligne {} : action non épinglée par SHA : {reference}",
                line.no
            ));
        }
    }
    found
}

// --- BR-RELEASE-011 : l'essai à blanc -----------------------------------------------------------

fn dry_run_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    let all = jobs(&lines);
    for job in &all {
        if job.name == "release-github" {
            let condition = value_of(job_keys(job), 4, "if:");
            if condition != Some(ONLY_REAL) {
                found.push(format!(
                    "release-github doit être sauté en essai (`if: {ONLY_REAL}`, lu : {condition:?})"
                ));
            }
        }
        if job.name != "publish" {
            continue;
        }
        for step in steps(job) {
            let whole: String = step
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let writes = [
                "HEARTH_RELEASES_",
                "hearth-release-signed",
                "/signed",
                "--confirm",
            ]
            .iter()
            .any(|word| whole.contains(word));
            let condition = value_of(&step, 8, "if:").unwrap_or_default();
            if writes && !condition.starts_with(ONLY_REAL) {
                found.push(format!(
                    "ligne {} : étape d'écriture sans `if: {ONLY_REAL}` (essai : aucune écriture)",
                    step[0].no
                ));
            }
            let dry = run_script(&step).is_some_and(|script| {
                script.iter().any(|l| l.contains("xtask release-upload"))
                    && !script.iter().any(|l| l.contains("--confirm"))
            });
            if dry && condition != "inputs.essai == true" {
                found.push(format!(
                    "ligne {} : le dépôt à blanc doit n'avoir lieu qu'en essai",
                    step[0].no
                ));
            }
        }
    }
    found
}

// --- autres promesses du flux -------------------------------------------------------------------

fn shape_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    let names: Vec<String> = jobs(&lines).into_iter().map(|job| job.name).collect();
    if names != JOBS {
        found.push(format!("jobs : {names:?}, attendus {JOBS:?}"));
    }
    let all = jobs(&lines);
    let expect = |name: &str, wanted: &[&str], found: &mut Vec<String>| {
        let got = all
            .iter()
            .find(|job| job.name == name)
            .map(needs)
            .unwrap_or_default();
        if got != wanted {
            found.push(format!("{name} : needs {got:?}, attendu {wanted:?}"));
        }
    };
    expect("preflight", &[], &mut found);
    expect("build-client", &["preflight"], &mut found);
    expect("build-agent", &["preflight"], &mut found);
    expect(
        "review",
        &["preflight", "build-client", "build-agent"],
        &mut found,
    );
    expect("release-github", &["preflight", "publish"], &mut found);
    // Concurrence : une seule exécution, jamais d'annulation de celle qui tourne.
    let concurrency = top(&lines, "concurrency:").unwrap_or_default();
    if !has(concurrency, 2, "group: hearth-publish")
        || !has(concurrency, 2, "cancel-in-progress: false")
    {
        found.push("concurrency : group hearth-publish, cancel-in-progress: false".to_owned());
    }
    // Aucun cache : une entrée de construction qui ne vient pas du commit.
    for line in &lines {
        if line.text.contains("rust-cache")
            || line.text.contains("actions/cache")
            || line.text.starts_with("cache:")
            || line.text.starts_with("cache-dependency-path:")
        {
            found.push(format!(
                "ligne {} : cache interdit ({})",
                line.no, line.text
            ));
        }
    }
    // `xtask` est compilé une fois (preflight) ; les jobs à clé ou à écriture n'ont ni Rust ni Node.
    for job in &all {
        let uses_toolchain = steps(job).iter().any(|step| {
            step.iter().any(|line| {
                line.text.starts_with("uses: dtolnay/rust-toolchain@")
                    || line.text.starts_with("uses: actions/setup-node@")
            })
        });
        if matches!(job.name.as_str(), "publish" | "release-github" | "review") && uses_toolchain {
            found.push(format!("{} ne doit installer ni Rust ni Node", job.name));
        }
        let compiles = steps(job).iter().any(|step| {
            run_script(step).is_some_and(|s| s.iter().any(|l| l.contains("cargo build -p xtask")))
        });
        if compiles != (job.name == "preflight") {
            found.push(format!(
                "{} : xtask se compile dans preflight seul",
                job.name
            ));
        }
    }
    found
}

fn aws_violations(text: &str) -> Vec<String> {
    let lines = parse(text);
    let mut found = Vec::new();
    let all = jobs(&lines);
    let Some(publish) = all.iter().find(|job| job.name == "publish") else {
        return vec!["job publish absent".to_owned()];
    };
    let keys = job_keys(publish);
    let version = value_of(keys, 6, "AWS_CLI_VERSION:").unwrap_or_default();
    let sum = value_of(keys, 6, "AWS_CLI_SHA256:").unwrap_or_default();
    let numbers = version
        .trim_matches('"')
        .split('.')
        .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    if !numbers || version.trim_matches('"').split('.').count() != 3 {
        found.push(format!(
            "AWS_CLI_VERSION doit être X.Y.Z exact (lu : {version})"
        ));
    }
    if sum.len() != 64 || !sum.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        found.push("AWS_CLI_SHA256 doit être une somme SHA-256 en hexadécimal".to_owned());
    }
    let script: Vec<String> = steps(publish)
        .iter()
        .filter(|step| step[0].text.contains("AWS CLI"))
        .flat_map(|step| run_script(step).unwrap_or_default())
        .collect();
    let position = |needle: &str| script.iter().position(|line| line.contains(needle));
    match (
        position("sha256sum --check"),
        position("unzip"),
        position("aws/install"),
        position("aws --version | grep"),
    ) {
        (Some(check), Some(unzip), Some(install), Some(_)) if check < unzip && unzip < install => {}
        _ => found.push(
            "l'AWS CLI doit être téléchargée, vérifiée (sha256sum --check) AVANT unzip et install, puis sa version relue"
                .to_owned(),
        ),
    }
    if !script.iter().any(|line| line.contains("${AWS_CLI_SHA256}")) {
        found.push("la somme vérifiée doit être AWS_CLI_SHA256".to_owned());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une copie abîmée : le remplacement DOIT avoir changé le texte, sinon le test ne prouverait rien.
    fn damaged(from: &str, to: &str) -> String {
        let text = read();
        assert!(text.contains(from), "la mutation `{from}` ne trouve rien");
        text.replacen(from, to, 1)
    }

    #[test]
    fn publish_has_only_the_manual_trigger() {
        assert_eq!(trigger_violations(&read()), Vec::<String>::new());
        for extra in [
            "push:",
            "pull_request:",
            "schedule:",
            "release:",
            "workflow_call:",
        ] {
            let text = damaged(
                "on:\n  workflow_dispatch:",
                &format!("on:\n  {extra}\n  workflow_dispatch:"),
            );
            assert!(!trigger_violations(&text).is_empty(), "{extra} accepté");
        }
        assert!(
            !trigger_violations(&damaged(
                "on:\n  workflow_dispatch:",
                "on: [workflow_dispatch]"
            ))
            .is_empty()
        );
        assert!(!trigger_violations(&damaged("default: true", "default: false")).is_empty());
        assert!(!trigger_violations(&damaged("required: true", "required: false")).is_empty());
    }

    #[test]
    fn secrets_appear_only_in_steps_of_the_release_job() {
        assert_eq!(secret_violations(&read()), Vec::<String>::new());
        // Au niveau du job (visible de toutes les étapes), dans un autre job, hors environnement.
        let job_level = damaged(
            "      HEARTH_NOTES: ${{ inputs.notes }}\n      # AWS CLI",
            "      HEARTH_NOTES: ${{ inputs.notes }}\n      LEAK: ${{ secrets.HEARTH_CLIENT_SIGNING_KEY }}\n      # AWS CLI",
        );
        assert!(!secret_violations(&job_level).is_empty());
        let other_job = damaged(
            "        run: target/debug/xtask release-preflight",
            "        run: target/debug/xtask release-preflight\n      - env:\n          X: ${{ secrets.HEARTH_AGENT_SIGNING_KEY }}\n        run: echo",
        );
        assert!(!secret_violations(&other_job).is_empty());
        assert!(!secret_violations(&damaged("    environment: release\n", "")).is_empty());
        assert!(
            !secret_violations(&damaged(
                "    needs: [preflight, build-client, build-agent, review]\n    runs-on: ubuntu-latest\n    environment",
                "    needs: [preflight, build-client, build-agent]\n    runs-on: ubuntu-latest\n    environment"
            ))
            .is_empty(),
            "publish sans review accepté"
        );
        assert!(
            !secret_violations(&damaged(
                "${{ secrets.HEARTH_AGENT_SIGNING_KEY }}",
                "${{ secrets.AUTRE }}"
            ))
            .is_empty()
        );
    }

    #[test]
    fn every_job_declares_its_permissions() {
        assert_eq!(permission_violations(&read()), Vec::<String>::new());
        // `review` sans permissions : héritage, donc refus.
        assert!(
            !permission_violations(&damaged(
                "    permissions: {}\n    timeout-minutes: 5",
                "    timeout-minutes: 5"
            ))
            .is_empty()
        );
        assert!(!permission_violations(&damaged("\npermissions: {}\n", "\n")).is_empty());
    }

    #[test]
    fn only_release_github_can_write_contents() {
        assert_eq!(write_violations(&read()), Vec::<String>::new());
        let in_publish = damaged(
            "    environment: release\n    permissions:\n      contents: read",
            "    environment: release\n    permissions:\n      contents: write",
        );
        assert!(!write_violations(&in_publish).is_empty());
        let other = damaged(
            "    permissions:\n      contents: read\n    timeout-minutes: 15\n    outputs:",
            "    permissions:\n      contents: read\n      id-token: write\n    timeout-minutes: 15\n    outputs:",
        );
        assert!(!write_violations(&other).is_empty());
    }

    #[test]
    fn checkout_never_persists_credentials() {
        assert_eq!(checkout_violations(&read()), Vec::<String>::new());
        let text = damaged("          persist-credentials: false\n", "");
        assert!(!checkout_violations(&text).is_empty());
    }

    #[test]
    fn every_checkout_uses_github_sha() {
        let text = read();
        assert_eq!(checkout_violations(&text), Vec::<String>::new());
        assert_eq!(text.matches("ref: ${{ github.sha }}").count(), 5);
        let moved = damaged(
            "          ref: ${{ github.sha }}\n",
            "          ref: main\n",
        );
        assert!(!checkout_violations(&moved).is_empty());
    }

    #[test]
    fn inputs_are_never_interpolated_in_run() {
        assert_eq!(input_violations(&read()), Vec::<String>::new());
        let inline = damaged(
            "        run: target/debug/xtask release-preflight",
            "        run: target/debug/xtask release-preflight --notes \"${{ inputs.notes }}\"",
        );
        assert!(!input_violations(&inline).is_empty());
        let block = damaged(
            "          chmod +x target/debug/xtask\n          target/debug/xtask ci-root-check",
            "          echo \"${{ inputs.notes }}\"\n          target/debug/xtask ci-root-check",
        );
        assert!(!input_violations(&block).is_empty());
    }

    #[test]
    fn the_dry_run_skips_the_upload_secrets_the_signed_artifact_and_the_github_release() {
        assert_eq!(dry_run_violations(&read()), Vec::<String>::new());
        // Le dépôt réel sans condition : en essai il écrirait.
        let upload = damaged(
            "      - name: Déposer au bucket\n        if: inputs.essai == false\n",
            "      - name: Déposer au bucket\n",
        );
        assert!(!dry_run_violations(&upload).is_empty());
        let signed = damaged(
            "      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2\n        if: inputs.essai == false && steps.resume.outcome != 'success'\n",
            "      - uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7.0.2\n",
        );
        assert!(!dry_run_violations(&signed).is_empty());
        let github = damaged(
            "    needs: [preflight, publish]\n    if: inputs.essai == false\n",
            "    needs: [preflight, publish]\n",
        );
        assert!(!dry_run_violations(&github).is_empty());
        let dry = damaged(
            "        if: inputs.essai == true\n        run: target/debug/xtask release-upload\n",
            "        run: target/debug/xtask release-upload\n",
        );
        assert!(!dry_run_violations(&dry).is_empty());
    }

    #[test]
    fn every_action_is_pinned_by_sha() {
        assert_eq!(pin_violations(&read()), Vec::<String>::new());
        let text = damaged(
            "actions/setup-node@949feb2413d6458794dcd2491c4babbbce0c15c1",
            "actions/setup-node@v7",
        );
        assert!(!pin_violations(&text).is_empty());
    }

    #[test]
    fn the_jobs_run_in_the_order_of_the_conception_without_cache_and_with_one_xtask_build() {
        assert_eq!(shape_violations(&read()), Vec::<String>::new());
        assert!(
            !shape_violations(&damaged(
                "cancel-in-progress: false",
                "cancel-in-progress: true"
            ))
            .is_empty()
        );
        assert!(
            !shape_violations(&damaged(
                "          node-version: \"22\"\n",
                "          node-version: \"22\"\n          cache: npm\n"
            ))
            .is_empty()
        );
        assert!(!shape_violations(&damaged(
            "    needs: [preflight, build-client, build-agent]\n    runs-on: ubuntu-latest\n    permissions: {}",
            "    needs: [preflight, build-client]\n    runs-on: ubuntu-latest\n    permissions: {}"
        ))
        .is_empty());
        // Rust installé dans le job à clés : refusé.
        let rust = damaged(
            "      - name: Outil xtask et garde de la racine\n",
            "      - uses: dtolnay/rust-toolchain@e2a55d2ffb04f378e9626c28d38b36d230d1e12f # master\n      - name: Outil xtask et garde de la racine\n",
        );
        assert!(!shape_violations(&rust).is_empty());
    }

    #[test]
    fn the_aws_cli_is_pinned_by_version_and_checksum_before_it_is_installed() {
        assert_eq!(aws_violations(&read()), Vec::<String>::new());
        assert!(
            !aws_violations(&damaged(
                "AWS_CLI_VERSION: \"2.38.1\"",
                "AWS_CLI_VERSION: \"2\""
            ))
            .is_empty()
        );
        let short = damaged(
            "b11ed416d91f4251da363a17706938eeaf0e520745754c104c0c1890dd8363c7",
            "b11ed416",
        );
        assert!(!aws_violations(&short).is_empty());
        // Vérification retirée, ou déplacée après l'installation.
        assert!(!aws_violations(&damaged("          echo \"${AWS_CLI_SHA256}  ${RUNNER_TEMP}/awscliv2.zip\" | sha256sum --check -\n", "")).is_empty());
        let late = damaged(
            "          echo \"${AWS_CLI_SHA256}  ${RUNNER_TEMP}/awscliv2.zip\" | sha256sum --check -\n          unzip -q \"$RUNNER_TEMP/awscliv2.zip\" -d \"$RUNNER_TEMP/awscli\"\n          sudo \"$RUNNER_TEMP/awscli/aws/install\" --update\n",
            "          unzip -q \"$RUNNER_TEMP/awscliv2.zip\" -d \"$RUNNER_TEMP/awscli\"\n          sudo \"$RUNNER_TEMP/awscli/aws/install\" --update\n          echo \"${AWS_CLI_SHA256}  ${RUNNER_TEMP}/awscliv2.zip\" | sha256sum --check -\n",
        );
        assert!(!aws_violations(&late).is_empty());
    }
}
