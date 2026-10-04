//! Contrôle de la version d'interface (BR-CONN-014) : le client annonce `X-Hearth-Api`, l'agent
//! accepte sa plage et répond `426 INCOMPATIBLE_VERSION` (qui dit qui doit se mettre à jour) hors
//! de la plage. Toute réponse des routes contrôlées porte `X-Hearth-Api-Range: min-max`.
//! `GET /hello` n'est pas contrôlée : elle sert à lire la plage avant de parler.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use hearth_proto::headers;
use hearth_proto::version::{API_MIN_SUPPORTED, API_VERSION};

use super::ApiError;
use crate::domain::compat;

pub async fn layer(request: Request, next: Next) -> Response {
    let mut response = match announced(&request) {
        Ok(version) => match compat::check(version) {
            Ok(()) => next.run(request).await,
            Err(incompatibility) => ApiError::from(incompatibility).into_response(),
        },
        Err(error) => error.into_response(),
    };
    if let Ok(range) = HeaderValue::from_str(&format!("{API_MIN_SUPPORTED}-{API_VERSION}")) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(headers::API_RANGE), range);
    }
    response
}

/// Version annoncée par le client ; absente ou illisible, c'est une erreur de validation.
fn announced(request: &Request) -> Result<u32, ApiError> {
    request
        .headers()
        .get(headers::API_VERSION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
        .ok_or_else(|| {
            ApiError::invalid(
                headers::API_VERSION,
                "L'en-tête X-Hearth-Api est requis : la version d'interface du client (un entier)",
            )
        })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    use super::*;

    fn request(value: Option<&str>) -> Request {
        let mut builder = HttpRequest::builder().uri("/x");
        if let Some(value) = value {
            builder = builder.header(headers::API_VERSION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn the_announced_version_is_a_plain_integer() {
        assert_eq!(announced(&request(Some("1"))).unwrap(), 1);
        assert_eq!(announced(&request(Some(" 2 "))).unwrap(), 2);
    }

    #[test]
    fn a_missing_or_unreadable_version_is_a_validation_error() {
        for value in [None, Some(""), Some("v1"), Some("-1"), Some("1.5")] {
            assert!(announced(&request(value)).is_err(), "{value:?}");
        }
    }
}
