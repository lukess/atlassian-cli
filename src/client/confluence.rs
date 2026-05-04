use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;

use crate::config::Config;

// ── Client ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ConfluenceClient {
    client: reqwest::Client,
    base_url: String,
    email: String,
    api_token: String,
}

// ── API Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct SpaceDescriptionValue {
    pub value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SpaceDescription {
    pub plain: Option<SpaceDescriptionValue>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Space {
    #[allow(dead_code)]
    pub id: u64,
    pub key: String,
    pub name: String,
    #[serde(rename = "type")]
    pub space_type: String,
    pub description: Option<SpaceDescription>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SpaceListResult {
    pub results: Vec<Space>,
    #[allow(dead_code)]
    pub start: u32,
    #[allow(dead_code)]
    pub limit: u32,
    #[allow(dead_code)]
    pub size: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageAuthor {
    #[serde(rename = "displayName")]
    pub display_name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageVersion {
    pub number: u32,
    pub when: Option<String>,
    pub by: Option<PageAuthor>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageSpace {
    pub key: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageBodyContent {
    /// The ADF document as a JSON string — must be parsed with serde_json::from_str.
    pub value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageBody {
    #[serde(rename = "atlas_doc_format")]
    pub atlas_doc_format: Option<PageBodyContent>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageAncestor {
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Page {
    pub id: String,
    pub title: String,
    #[allow(dead_code)]
    pub status: Option<String>,
    pub version: Option<PageVersion>,
    pub space: Option<PageSpace>,
    pub body: Option<PageBody>,
    #[serde(default)]
    pub ancestors: Vec<PageAncestor>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PageListResult {
    pub results: Vec<Page>,
    #[allow(dead_code)]
    pub start: u32,
    #[allow(dead_code)]
    pub limit: u32,
    pub size: u32,
}

/// Result from /wiki/rest/api/search — each result wraps a `content` object.
#[derive(Debug, Deserialize)]
pub struct SearchItem {
    pub content: Option<Page>,
    #[allow(dead_code)]
    pub title: String,
    #[serde(rename = "lastModified")]
    #[allow(dead_code)]
    pub last_modified: Option<String>,
    #[allow(dead_code)]
    pub excerpt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchApiResult {
    pub results: Vec<SearchItem>,
    #[allow(dead_code)]
    pub start: u32,
    #[allow(dead_code)]
    pub limit: u32,
    #[serde(rename = "totalSize")]
    #[allow(dead_code)]
    pub total_size: u32,
}

// ── ConfluenceClient impl ──────────────────────────────────────────────────────

impl ConfluenceClient {
    pub fn new(config: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        // confluence_api_token → CONFLUENCE_API_TOKEN env var → falls back to Jira jira_api_token
        let api_token = config
            .confluence_api_token
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| config.jira_api_token.clone());

        Ok(Self {
            client,
            base_url: config.confluence_api_base_url(),
            email: config.email.clone(),
            api_token,
        })
    }

    fn auth(&self) -> (&str, &str) {
        (&self.email, &self.api_token)
    }

    async fn check_response(&self, resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&msg)
                .ok()
                .and_then(|v| v["message"].as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| if msg.trim_start().starts_with('<') {
                    format!("HTTP {}", status.as_u16())
                } else {
                    msg
                });
            let hint = if status == 404 && message.contains("No space with key") {
                "\n  Hint: Use `atlassian confluence space list --plain` to find valid space keys"
            } else {
                ""
            };
            return Err(anyhow!("Confluence API error {}: {}{}", status.as_u16(), message, hint));
        }
        Ok(resp)
    }

    pub async fn list_spaces(&self, limit: u32, start: u32, space_type: Option<&str>) -> Result<SpaceListResult> {
        let url = format!("{}/space", self.base_url);
        let mut params = vec![
            ("limit".to_string(), limit.to_string()),
            ("start".to_string(), start.to_string()),
            ("expand".to_string(), "description.plain".to_string()),
        ];
        if let Some(t) = space_type {
            params.push(("type".to_string(), t.to_string()));
        }
        let resp = self
            .client
            .get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&params)
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_pages(&self, space_key: &str, limit: u32, start: u32) -> Result<PageListResult> {
        let url = format!("{}/content", self.base_url);
        let resp = self
            .client
            .get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[
                ("spaceKey", space_key.to_string()),
                ("type", "page".to_string()),
                ("limit", limit.to_string()),
                ("start", start.to_string()),
                ("expand", "version,space".to_string()),
            ])
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_page(&self, id: &str) -> Result<Page> {
        let url = format!("{}/content/{}", self.base_url, id);
        let resp = self
            .client
            .get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("expand", "body.atlas_doc_format,version,space,ancestors")])
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_child_pages(&self, id: &str) -> Result<Vec<Page>> {
        let url = format!("{}/content/{}/child/page", self.base_url, id);
        let resp = self
            .client
            .get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("limit", "200"), ("expand", "version")])
            .send()
            .await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json::<PageListResult>().await?.results)
    }

    /// Fetch recently modified pages across all spaces (no query filter).
    pub async fn list_recent_pages(&self, limit: u32) -> Result<PageListResult> {
        let cql = "type=page ORDER BY lastModified DESC";
        let url = format!("{}/search", self.base_url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[
                ("cql", cql),
                ("limit", &limit.to_string()),
                ("expand", "content.space,content.version"),
            ])
            .send().await?;
        let resp = self.check_response(resp).await?;
        let sr: SearchApiResult = resp.json().await?;
        let pages: Vec<Page> = sr.results.into_iter().filter_map(|i| i.content).collect();
        let size = pages.len() as u32;
        Ok(PageListResult { results: pages, start: sr.start, limit: sr.limit, size })
    }

    /// Search pages using CQL via /wiki/rest/api/search.
    /// Supports title substring (~), space filter, and full-text search.
    pub async fn search_pages(&self, query: &str, space_key: Option<&str>, limit: u32, start: u32) -> Result<PageListResult> {
        let cql = match space_key {
            Some(sk) => format!("title~\"{}\" AND space=\"{}\" AND type=page ORDER BY lastModified DESC", query, sk),
            None    => format!("title~\"{}\" AND type=page ORDER BY lastModified DESC", query),
        };
        let url = format!("{}/search", self.base_url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[
                ("cql", cql.as_str()),
                ("limit", &limit.to_string()),
                ("start", &start.to_string()),
                ("expand", "content.space,content.version"),
            ])
            .send().await?;
        let resp = self.check_response(resp).await?;
        let sr: SearchApiResult = resp.json().await?;
        // Map SearchItem → Page (content field already has the Page struct)
        let pages: Vec<Page> = sr.results.into_iter()
            .filter_map(|item| item.content)
            .collect();
        let size = pages.len() as u32;
        Ok(PageListResult { results: pages, start: sr.start, limit: sr.limit, size })
    }
}
