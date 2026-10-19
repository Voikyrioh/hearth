//! Services supervisés retenus et services adoptés, en SQLite (`supervised_services`, HRT-63,
//! ADR-0037 ; `adopted_services`, HRT-79, ADR-0043).
//!
//! `last_view_json` est un objet à nous, fabriqué ici depuis une `Observation` : il ne contient NI
//! valeur de variable, NI nom de variable, NI commande (l'observation retenue en est déjà dépourvue
//! et ce DTO n'a pas de champ pour les recevoir). C'est la seule table de cette migration que la
//! tranche 2 écrit.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use time::OffsetDateTime;

use super::convert::{format_date, parse_date, storage};
use crate::application::ports::{AdoptedRecord, ServiceRepo, StoreError, SupervisedRecord};
use crate::domain::services::image::ImageName;
use crate::domain::services::{
    MAX_MOUNTS, MAX_PORTS, MountInfo, MountType, Observation, PortBinding, PortProtocol, PortScope,
    RestartPolicy, RunState, ServiceKey, UnitName,
};

const RESOURCE: &str = "supervised_services";
const ADOPTED: &str = "adopted_services";

pub struct SqliteServiceRepo {
    pool: SqlitePool,
}

impl SqliteServiceRepo {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Le bassin, pour les autres dépôts de la même famille (`intents_repo`).
    pub(super) fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// La forme stockée d'un service supervisé : ses réglages, rien de ce qui est secret.
#[derive(Serialize, Deserialize)]
struct StoredView {
    id: String,
    name: String,
    created_at: Option<String>,
    image_ref: Option<String>,
    image_tag: Option<String>,
    image_short_id: String,
    ports: Vec<StoredPort>,
    ports_omitted: u32,
    host_network: bool,
    mounts: Vec<StoredMount>,
    mounts_omitted: u32,
    restart: String,
    auto_remove: bool,
    compose_project: Option<String>,
    stop_timeout_s: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct StoredPort {
    host_port: u16,
    container_port: u16,
    protocol: String,
    local_only: bool,
}

#[derive(Serialize, Deserialize)]
struct StoredMount {
    folder: bool,
    source: String,
    target: String,
    read_only: bool,
}

fn to_stored(observation: &Observation) -> Result<StoredView, StoreError> {
    let created_at = observation
        .created_at
        .map(|date| format_date(RESOURCE, date))
        .transpose()?;
    Ok(StoredView {
        id: observation.id.clone(),
        name: observation.name.clone(),
        created_at,
        image_ref: observation.image.reference.clone(),
        image_tag: observation.image.tag.clone(),
        image_short_id: observation.image.short_id.clone(),
        ports: observation
            .ports
            .iter()
            .map(|port| StoredPort {
                host_port: port.host_port,
                container_port: port.container_port,
                protocol: match port.protocol {
                    PortProtocol::Tcp => "tcp",
                    PortProtocol::Udp => "udp",
                    PortProtocol::Sctp => "sctp",
                }
                .to_owned(),
                local_only: port.scope == PortScope::Local,
            })
            .collect(),
        ports_omitted: observation.ports_omitted,
        host_network: observation.host_network,
        mounts: observation
            .mounts
            .iter()
            .map(|mount| StoredMount {
                folder: mount.kind == MountType::Folder,
                source: mount.source.clone(),
                target: mount.target.clone(),
                read_only: mount.read_only,
            })
            .collect(),
        mounts_omitted: observation.mounts_omitted,
        restart: match observation.restart {
            RestartPolicy::None => "none",
            RestartPolicy::Always => "always",
            RestartPolicy::UnlessStopped => "unless_stopped",
            RestartPolicy::OnFailure => "on_failure",
        }
        .to_owned(),
        auto_remove: observation.auto_remove,
        compose_project: observation.compose_project.clone(),
        stop_timeout_s: observation.stop_timeout_s,
    })
}

/// Le service retenu, tel qu'il est listé quand son conteneur n'existe plus : arrêté, sans variable
/// ni commande.
fn from_stored(stored: StoredView) -> Result<Observation, StoreError> {
    let created_at = stored
        .created_at
        .as_deref()
        .map(|value| parse_date(RESOURCE, value))
        .transpose()?;
    Ok(Observation {
        id: stored.id,
        name: stored.name,
        created_at,
        run_state: RunState::Exited,
        exit_code: None,
        started_at: None,
        finished_at: None,
        image: ImageName {
            reference: stored.image_ref,
            tag: stored.image_tag,
            short_id: stored.image_short_id,
        },
        ports: stored
            .ports
            .into_iter()
            .take(MAX_PORTS)
            .filter_map(|port| {
                let protocol = match port.protocol.as_str() {
                    "tcp" => PortProtocol::Tcp,
                    "udp" => PortProtocol::Udp,
                    "sctp" => PortProtocol::Sctp,
                    _ => return None,
                };
                Some(PortBinding {
                    host_port: port.host_port,
                    container_port: port.container_port,
                    protocol,
                    scope: if port.local_only {
                        PortScope::Local
                    } else {
                        PortScope::All
                    },
                })
            })
            .collect(),
        ports_omitted: stored.ports_omitted,
        host_network: stored.host_network,
        mounts: stored
            .mounts
            .into_iter()
            .take(MAX_MOUNTS)
            .map(|mount| MountInfo {
                kind: if mount.folder {
                    MountType::Folder
                } else {
                    MountType::Volume
                },
                source: mount.source,
                target: mount.target,
                read_only: mount.read_only,
            })
            .collect(),
        mounts_omitted: stored.mounts_omitted,
        restart: match stored.restart.as_str() {
            "always" => RestartPolicy::Always,
            "unless_stopped" => RestartPolicy::UnlessStopped,
            "on_failure" => RestartPolicy::OnFailure,
            _ => RestartPolicy::None,
        },
        auto_remove: stored.auto_remove,
        compose_project: stored.compose_project,
        env_names: Vec::new(),
        env_omitted: 0,
        command: None,
        stop_timeout_s: stored.stop_timeout_s,
        memory_limit_bytes: None,
        cpu_millis: None,
        tty: false,
        log_driver: None,
        hearth_key: None,
    })
}

#[async_trait]
impl ServiceRepo for SqliteServiceRepo {
    async fn load_supervised(&self) -> Result<Vec<SupervisedRecord>, StoreError> {
        let rows = sqlx::query!(
            "SELECT unit, last_view_json, last_seen_at FROM supervised_services ORDER BY unit"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            // Une ligne illisible est omise et tracée, jamais fatale : le service reviendra avec
            // son prochain démarrage.
            let record = (|| {
                let unit = UnitName::parse(&row.unit).ok_or_else(|| StoreError::Unavailable {
                    resource: RESOURCE,
                    message: "nom d'unité illisible".to_owned(),
                })?;
                let stored: StoredView =
                    serde_json::from_str(&row.last_view_json).map_err(|_| {
                        StoreError::Unavailable {
                            resource: RESOURCE,
                            message: "réglages illisibles".to_owned(),
                        }
                    })?;
                Ok::<_, StoreError>(SupervisedRecord {
                    unit,
                    observation: from_stored(stored)?,
                    last_seen_at: parse_date(RESOURCE, &row.last_seen_at)?,
                })
            })();
            match record {
                Ok(record) => records.push(record),
                Err(error) => tracing::warn!(%error, "service supervisé retenu illisible, omis"),
            }
        }
        Ok(records)
    }

    async fn save_supervised(&self, record: &SupervisedRecord) -> Result<(), StoreError> {
        let json = serde_json::to_string(&to_stored(&record.observation)?).map_err(|error| {
            StoreError::Unavailable {
                resource: RESOURCE,
                message: error.to_string(),
            }
        })?;
        let key = format!("u_{}", record.unit);
        let unit = record.unit.as_str();
        let name = record.observation.name.as_str();
        let seen = format_date(RESOURCE, record.last_seen_at)?;
        sqlx::query!(
            "INSERT INTO supervised_services (service_key, unit, name, last_view_json, last_seen_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (service_key) DO UPDATE SET
                 name = excluded.name,
                 last_view_json = excluded.last_view_json,
                 last_seen_at = excluded.last_seen_at",
            key,
            unit,
            name,
            json,
            seen
        )
        .execute(&self.pool)
        .await
        .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn forget_supervised(&self, unit: &UnitName) -> Result<(), StoreError> {
        let key = format!("u_{unit}");
        sqlx::query!("DELETE FROM supervised_services WHERE service_key = ?", key)
            .execute(&self.pool)
            .await
            .map_err(storage(RESOURCE))?;
        Ok(())
    }

    async fn load_adopted(&self) -> Result<Vec<AdoptedRecord>, StoreError> {
        let rows = sqlx::query!(
            "SELECT service_key, adopted_by, adopted_at, unconfirmed_since FROM adopted_services ORDER BY service_key"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage(ADOPTED))?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            // Une ligne illisible est omise et tracée : son service reste non publié.
            let record = ServiceKey::parse(&row.service_key)
                .ok_or_else(|| StoreError::Unavailable {
                    resource: ADOPTED,
                    message: "clé illisible".to_owned(),
                })
                .and_then(|key| {
                    Ok(AdoptedRecord {
                        key,
                        by: row.adopted_by.clone(),
                        at: parse_date(ADOPTED, &row.adopted_at)?,
                        unconfirmed_since: row
                            .unconfirmed_since
                            .as_deref()
                            .map(|since| parse_date(ADOPTED, since))
                            .transpose()?,
                    })
                });
            match record {
                Ok(record) => records.push(record),
                Err(error) => tracing::warn!(%error, "adoption illisible, omise"),
            }
        }
        Ok(records)
    }

    async fn forget_adopted(&self, key: &ServiceKey) -> Result<(), StoreError> {
        let key = key.as_str();
        sqlx::query!("DELETE FROM adopted_services WHERE service_key = ?", key)
            .execute(&self.pool)
            .await
            .map_err(storage(ADOPTED))?;
        Ok(())
    }

    async fn mark_unconfirmed(
        &self,
        marks: &[(ServiceKey, OffsetDateTime)],
        since: OffsetDateTime,
    ) -> Result<Vec<(ServiceKey, OffsetDateTime)>, StoreError> {
        let since_text = format_date(ADOPTED, since)?;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(storage(ADOPTED))?;
        let mut effective = Vec::with_capacity(marks.len());
        for (key, adopted_at) in marks {
            let adopted_at = format_date(ADOPTED, *adopted_at)?;
            // Dynamique : la date « depuis » existante l'emporte (`COALESCE`), seule une clé `u_` est
            // touchée (la base le garantit aussi).
            sqlx::query(
                "UPDATE adopted_services SET unconfirmed_since = COALESCE(unconfirmed_since, ?)
                  WHERE service_key = ? AND adopted_at = ?",
            )
            .bind(&since_text)
            .bind(key.as_str())
            .bind(&adopted_at)
            .execute(&mut *tx)
            .await
            .map_err(storage(ADOPTED))?;
            let row: Option<Option<String>> = sqlx::query_scalar(
                "SELECT unconfirmed_since FROM adopted_services WHERE service_key = ? AND adopted_at = ?",
            )
            .bind(key.as_str())
            .bind(&adopted_at)
            .fetch_optional(&mut *tx)
            .await
            .map_err(storage(ADOPTED))?;
            if let Some(Some(text)) = row {
                effective.push((key.clone(), parse_date(ADOPTED, &text)?));
            }
        }
        tx.commit().await.map_err(storage(ADOPTED))?;
        Ok(effective)
    }
}
