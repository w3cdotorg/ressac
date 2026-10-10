//! archive.org availability API: the closest snapshot of a site around a date.

use std::time::Duration;

use chrono::{DateTime, Utc};
use url::Url;

use crate::error::{Error, Result};
use crate::source::USER_AGENT;

#[derive(Clone)]
pub struct Wayback {
    client: reqwest::Client,
    base: String,
}

impl Wayback {
    /// `base` is `https://archive.org` in production, a mock server in tests.
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self {
            client,
            base: base.into().trim_end_matches('/').to_string(),
        })
    }

    pub async fn closest(
        &self,
        site: &str,
        around: Option<DateTime<Utc>>,
    ) -> Result<Option<String>> {
        let mut url = Url::parse(&format!("{}/wayback/available", self.base))
            .map_err(|e| Error::Invalid(format!("bad Wayback base URL: {e}")))?;
        url.query_pairs_mut().append_pair("url", site);
        if let Some(t) = around {
            url.query_pairs_mut()
                .append_pair("timestamp", &t.format("%Y%m%d").to_string());
        }
        let body = self
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let v: serde_json::Value = serde_json::from_slice(&body)?;
        let snap = &v["archived_snapshots"]["closest"];
        Ok(if snap["available"].as_bool() == Some(true) {
            snap["url"].as_str().map(str::to_owned)
        } else {
            None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FOUND: &str = r#"{"url":"bibliobsession.net","archived_snapshots":{"closest":{"status":"200","available":true,"url":"http://web.archive.org/web/20210612000000/https://bibliobsession.net/","timestamp":"20210612000000"}}}"#;

    #[tokio::test]
    async fn returns_the_closest_snapshot() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/wayback/available"))
            .and(query_param("url", "https://bibliobsession.net/"))
            .and(query_param("timestamp", "20190402"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(FOUND, "application/json"))
            .mount(&server)
            .await;
        let around = "2019-04-02T10:00:00Z".parse().unwrap();
        let url = Wayback::new(format!("{}/", server.uri()))
            .unwrap()
            .closest("https://bibliobsession.net/", Some(around))
            .await
            .unwrap();
        assert_eq!(
            url.as_deref(),
            Some("http://web.archive.org/web/20210612000000/https://bibliobsession.net/")
        );
    }

    #[tokio::test]
    async fn no_snapshot_is_none() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(r#"{"archived_snapshots":{}}"#, "application/json"),
            )
            .mount(&server)
            .await;
        let url = Wayback::new(server.uri())
            .unwrap()
            .closest("https://x.org/", None)
            .await
            .unwrap();
        assert_eq!(url, None);
    }

    #[tokio::test]
    async fn server_error_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        assert!(
            Wayback::new(server.uri())
                .unwrap()
                .closest("https://x.org/", None)
                .await
                .is_err()
        );
    }
}
