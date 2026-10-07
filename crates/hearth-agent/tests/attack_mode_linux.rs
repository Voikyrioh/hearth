//! Le mode attaque sur le vrai noyau Linux (HRT-25, ADR-0025, BR-TRUST-020, 021) : le vrai
//! `/proc/sys/kernel/random/boot_id` et le vrai `/proc/uptime`, un vrai agent qui s'arrête et repart.
//! Vide hors Linux. Dans un conteneur, le `boot_id` est celui de l'hôte : redémarrer le conteneur
//! n'ouvre pas de fenêtre (le sens voulu), la fenêtre d'un redémarrage de la machine se vérifie à la
//! main sur la forge.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use std::sync::Arc;

use hearth_agent::application::ports::BootInfo;
use hearth_agent::infrastructure::system::ProcBootInfo;
use support::probe::metering;
use support::{env, https};
use time::Duration;

async fn scalar(env: &support::Env, sql: &'static str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(env.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_real_agent_restarted_on_the_same_kernel_boot_keeps_the_mode_and_opens_no_window() {
    let env = env().await;
    sqlx::query(
        "UPDATE attack_mode SET active = 1, activation_id = 'ACT1',
                activated_at = '2026-10-07T01:00:00.000Z', activated_by = 'marie' WHERE id = 1",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    for _ in 0..3 {
        let agent = https::start_booted(&env, metering(), Arc::new(ProcBootInfo::new())).await;
        agent.stop_gracefully().await;
    }
    assert_eq!(scalar(&env, "SELECT active FROM attack_mode").await, 1);
    assert_eq!(
        scalar(
            &env,
            "SELECT COUNT(*) FROM audit_events WHERE action = 'attack_mode.suspend'"
        )
        .await,
        0,
        "un redémarrage du service n'ouvre aucune fenêtre"
    );
    let noted: Option<String> = sqlx::query_scalar("SELECT last_boot_id FROM attack_mode")
        .fetch_one(env.db.pool())
        .await
        .unwrap();
    assert_eq!(noted, ProcBootInfo::new().boot_id());
}

#[tokio::test]
async fn a_real_agent_started_on_another_kernel_boot_suspends_only_if_the_machine_just_started() {
    let env = env().await;
    sqlx::query(
        "UPDATE attack_mode SET active = 1, activation_id = 'ACT1',
                activated_at = '2026-10-07T01:00:00.000Z', activated_by = 'marie',
                last_boot_id = 'another-boot-of-the-machine' WHERE id = 1",
    )
    .execute(env.db.pool())
    .await
    .unwrap();
    let before = ProcBootInfo::new().uptime();
    let agent = https::start_booted(&env, metering(), Arc::new(ProcBootInfo::new())).await;
    let after = ProcBootInfo::new().uptime();
    agent.stop_gracefully().await;
    let suspended = scalar(
        &env,
        "SELECT COUNT(*) FROM audit_events WHERE action = 'attack_mode.suspend'",
    )
    .await;
    let window = Duration::minutes(30);
    // Le temps écoulé lu au lancement est entre `before` et `after` : la décision est sans ambiguïté
    // sauf à la seconde près de la trentième minute.
    if after < window {
        assert_eq!(
            suspended, 1,
            "la machine vient de démarrer : fenêtre ouverte"
        );
    } else if before >= window {
        assert_eq!(
            suspended, 0,
            "la machine tourne depuis longtemps : pas de fenêtre"
        );
    }
}
