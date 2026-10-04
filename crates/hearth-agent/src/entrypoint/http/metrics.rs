//! `GET /machine` et `GET /metrics/history` : identité de la machine et historique des mesures,
//! pour tous les rôles (BR-DASH-013).

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use hearth_proto::api::machine::MachineResponse;
use hearth_proto::api::metrics::HistoryResponse;
use serde::Deserialize;

use super::{ApiError, AppState};
use crate::domain::metrics::HistoryWindow;
use crate::entrypoint::metrics_wire;

/// `GET /api/v1/machine`.
pub async fn machine(State(state): State<AppState>) -> Result<Json<MachineResponse>, ApiError> {
    let identity = state
        .metrics
        .identity()
        .await
        .map_err(|error| ApiError::internal(&error))?;
    Ok(Json(metrics_wire::machine(&identity)))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    window: Option<String>,
}

/// La fenêtre demandée : `5m` si le paramètre est absent, une erreur de validation s'il est
/// inconnu.
fn window_of(query: &HistoryQuery) -> Result<HistoryWindow, ApiError> {
    match query.window.as_deref() {
        None => Ok(HistoryWindow::default()),
        Some(label) => HistoryWindow::parse(label)
            .ok_or_else(|| ApiError::invalid("window", "La fenêtre doit valoir 1m, 5m ou 1h")),
    }
}

/// `GET /api/v1/metrics/history?window=1m|5m|1h`.
pub async fn history(
    State(state): State<AppState>,
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> Result<Json<HistoryResponse>, ApiError> {
    let Query(query) = query
        .map_err(|_| ApiError::invalid("window", "Les paramètres de la requête sont illisibles"))?;
    let window = window_of(&query)?;
    let samples = state.metrics.history(window);
    Ok(Json(HistoryResponse {
        window: metrics_wire::window_to_wire(window),
        step_s: window.step_s(),
        samples: samples.iter().map(metrics_wire::sample).collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(window: Option<&str>) -> HistoryQuery {
        HistoryQuery {
            window: window.map(str::to_owned),
        }
    }

    #[test]
    fn the_default_window_is_five_minutes() {
        assert_eq!(
            window_of(&query(None)).ok(),
            Some(HistoryWindow::FiveMinutes)
        );
    }

    #[test]
    fn the_three_documented_windows_are_accepted_and_nothing_else() {
        assert_eq!(
            window_of(&query(Some("1m"))).ok(),
            Some(HistoryWindow::OneMinute)
        );
        assert_eq!(
            window_of(&query(Some("5m"))).ok(),
            Some(HistoryWindow::FiveMinutes)
        );
        assert_eq!(
            window_of(&query(Some("1h"))).ok(),
            Some(HistoryWindow::OneHour)
        );
        for bad in ["", "2m", "60m", "1H"] {
            assert!(window_of(&query(Some(bad))).is_err(), "{bad:?}");
        }
    }
}
