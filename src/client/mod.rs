use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

pub mod confluence;

use crate::config::Config;

#[derive(Clone)]
pub struct JiraClient {
    client: reqwest::Client,
    base_url: String,
    email: String,
    api_token: String,
}

// ── API Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Comment {
    pub id: String,
    pub author: Option<User>,
    #[serde(rename = "created")]
    pub created: Option<String>,
    pub body: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommentsField {
    pub comments: Vec<Comment>,
    pub total: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct User {
    #[serde(rename = "accountId")]
    pub account_id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "emailAddress", default)]
    pub email_address: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StatusCategory {
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Status {
    pub name: String,
    #[serde(rename = "statusCategory")]
    pub category: Option<StatusCategory>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Priority {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueType {
    pub name: String,
    #[serde(rename = "iconUrl", default)]
    pub icon_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Component {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Version {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Sprint {
    pub id: i64,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Resolution {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueLinkType {
    pub name: String,
    pub inward: String,
    pub outward: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LinkedIssueFields {
    pub summary: String,
    pub status: Option<Status>,
    #[serde(rename = "issuetype")]
    pub issue_type: Option<IssueType>,
    pub priority: Option<Priority>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LinkedIssue {
    pub id: String,
    pub key: String,
    pub fields: LinkedIssueFields,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueLink {
    pub id: String,
    #[serde(rename = "type")]
    pub link_type: IssueLinkType,
    #[serde(rename = "inwardIssue")]
    pub inward_issue: Option<LinkedIssue>,
    #[serde(rename = "outwardIssue")]
    pub outward_issue: Option<LinkedIssue>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub content: String,
    pub size: u64,
    pub author: Option<User>,
    pub created: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IssueFields {
    pub summary: String,
    pub status: Option<Status>,
    pub priority: Option<Priority>,
    pub assignee: Option<User>,
    pub reporter: Option<User>,
    pub resolution: Option<Resolution>,
    #[serde(rename = "issuetype")]
    pub issue_type: Option<IssueType>,
    pub description: Option<Value>,
    pub created: Option<String>,
    pub updated: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(rename = "fixVersions", default)]
    pub fix_versions: Vec<Version>,
    #[serde(default)]
    pub attachment: Vec<Attachment>,
    pub parent: Option<Box<Issue>>,
    pub subtasks: Option<Vec<Issue>>,
    #[serde(rename = "issuelinks", default)]
    pub issue_links: Vec<IssueLink>,
    #[serde(rename = "customfield_10020")]
    pub sprint: Option<Value>,
    pub comment: Option<CommentsField>,
    /// Captures all other custom fields (customfield_XXXXX) not explicitly declared.
    #[serde(flatten)]
    pub custom: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Issue {
    pub id: String,
    pub key: String,
    pub fields: IssueFields,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchResult {
    #[serde(rename = "nextPageToken", default)]
    pub next_page_token: Option<String>,
    #[serde(default)]
    pub total: u32,
    pub issues: Vec<Issue>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Project {
    #[allow(dead_code)]
    pub id: String,
    pub key: String,
    pub name: String,
    #[serde(rename = "projectTypeKey", default)]
    pub project_type: String,
    #[serde(rename = "isPrivate", default)]
    #[allow(dead_code)]
    pub is_private: bool,
    pub lead: Option<User>,
}

#[derive(Debug, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TransitionsResult {
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IssueLinkTypeInfo {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub inward: String,
    pub outward: String,
}

#[derive(Debug, Deserialize)]
struct IssueLinkTypesResult {
    #[serde(rename = "issueLinkTypes")]
    issue_link_types: Vec<IssueLinkTypeInfo>,
}

// ── JiraClient impl ────────────────────────────────────────────────────────────

impl JiraClient {
    pub fn new(config: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        debug!("JiraClient: base_url={}", config.api_base_url());
        debug!("JiraClient: email={}", config.email);
        debug!("JiraClient: api_token={}...{}", &config.jira_api_token[..8.min(config.jira_api_token.len())], &config.jira_api_token[config.jira_api_token.len().saturating_sub(4)..]);

        Ok(Self {
            client,
            base_url: config.api_base_url(),
            email: config.email.clone(),
            api_token: config.jira_api_token.clone(),
        })
    }

    fn auth(&self) -> (&str, &str) {
        (&self.email, &self.api_token)
    }

    async fn check_response(&self, resp: reqwest::Response) -> Result<reqwest::Response> {
        let status = resp.status();
        debug!("HTTP {} {}", status.as_u16(), resp.url());
        if !status.is_success() {
            let msg = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&msg)
                .ok()
                .and_then(|v| {
                    v["errorMessages"].as_array()
                        .and_then(|a| a.first())
                        .and_then(|s| s.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| v["message"].as_str().map(|s| s.to_string()))
                })
                .unwrap_or(msg);
            return Err(anyhow!("API error {}: {}", status.as_u16(), message));
        }
        Ok(resp)
    }

    pub async fn myself(&self) -> Result<User> {
        let url = format!("{}/myself", self.base_url);
        debug!("GET {}", url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn search_issues(&self, jql: &str, next_page_token: Option<&str>, max_results: u32) -> Result<SearchResult> {
        let url = format!("{}/search/jql", self.base_url);
        debug!("GET {} jql={:?} max_results={}", url, jql, max_results);
        let mut params = vec![
            ("jql", jql.to_string()),
            ("maxResults", max_results.to_string()),
            ("fields", "*all".to_string()),
        ];
        if let Some(token) = next_page_token {
            params.push(("nextPageToken", token.to_string()));
        }
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&params)
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_issue(&self, key: &str) -> Result<Issue> {
        let url = format!("{}/issue/{}", self.base_url, key);
        debug!("GET {}", url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("fields", "*all")])
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_transitions(&self, key: &str) -> Result<Vec<Transition>> {
        let url = format!("{}/issue/{}/transitions", self.base_url, key);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .send().await?;
        let resp = self.check_response(resp).await?;
        let result: TransitionsResult = resp.json().await?;
        Ok(result.transitions)
    }

    pub async fn transition_issue(&self, key: &str, transition_id: &str) -> Result<()> {
        let url = format!("{}/issue/{}/transitions", self.base_url, key);
        let body = serde_json::json!({
            "transition": { "id": transition_id }
        });
        let resp = self.client.post(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&body)
            .send().await?;
        self.check_response(resp).await?;
        Ok(())
    }

    pub async fn create_issue(
        &self,
        project_key: &str,
        issue_type: &str,
        summary: &str,
        description: Option<&str>,
        assignee_id: Option<&str>,
        priority: Option<&str>,
        labels: &[String],
        components: &[String],
        parent_key: Option<&str>,
    ) -> Result<Issue> {
        let url = format!("{}/issue", self.base_url);

        let desc_value = description.map(|d| serde_json::json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{ "type": "text", "text": d }]
            }]
        }));

        let mut fields = serde_json::json!({
            "project": { "key": project_key },
            "issuetype": { "name": issue_type },
            "summary": summary,
        });

        if let Some(desc) = desc_value {
            fields["description"] = desc;
        }
        if let Some(aid) = assignee_id {
            fields["assignee"] = serde_json::json!({ "accountId": aid });
        }
        if let Some(p) = priority {
            fields["priority"] = serde_json::json!({ "name": p });
        }
        if !labels.is_empty() {
            fields["labels"] = serde_json::json!(labels);
        }
        if !components.is_empty() {
            let comps: Vec<_> = components.iter().map(|c| serde_json::json!({"name": c})).collect();
            fields["components"] = serde_json::json!(comps);
        }
        if let Some(pk) = parent_key {
            fields["parent"] = serde_json::json!({ "key": pk });
        }

        let body = serde_json::json!({ "fields": fields });
        let resp = self.client.post(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&body)
            .send().await?;
        let resp = self.check_response(resp).await?;
        let created: Value = resp.json().await?;
        let key = created["key"].as_str().ok_or_else(|| anyhow!("No key in create response"))?;
        self.get_issue(key).await
    }

    pub async fn edit_issue(
        &self,
        key: &str,
        summary: Option<&str>,
        description: Option<&str>,
        priority: Option<&str>,
        assignee: Option<&str>,
        add_labels: &[String],
        remove_labels: &[String],
        add_components: &[String],
        remove_components: &[String],
        add_fix_versions: &[String],
        remove_fix_versions: &[String],
    ) -> Result<()> {
        let url = format!("{}/issue/{}", self.base_url, key);

        let mut fields = serde_json::json!({});
        if let Some(s) = summary {
            fields["summary"] = serde_json::json!(s);
        }
        if let Some(d) = description {
            fields["description"] = serde_json::json!({
                "type": "doc", "version": 1,
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": d}]}]
            });
        }
        if let Some(p) = priority {
            fields["priority"] = serde_json::json!({"name": p});
        }
        if let Some(a) = assignee {
            fields["assignee"] = match a {
                "x" => serde_json::json!(null),
                "default" => serde_json::json!({"accountId": "-1"}),
                id => serde_json::json!({"accountId": id}),
            };
        }

        let mut update = serde_json::json!({});
        if !add_labels.is_empty() || !remove_labels.is_empty() {
            let ops: Vec<Value> = add_labels.iter().map(|l| serde_json::json!({"add": l}))
                .chain(remove_labels.iter().map(|l| serde_json::json!({"remove": l})))
                .collect();
            update["labels"] = serde_json::json!(ops);
        }
        if !add_components.is_empty() || !remove_components.is_empty() {
            let ops: Vec<Value> = add_components.iter().map(|c| serde_json::json!({"add": {"name": c}}))
                .chain(remove_components.iter().map(|c| serde_json::json!({"remove": {"name": c}})))
                .collect();
            update["components"] = serde_json::json!(ops);
        }
        if !add_fix_versions.is_empty() || !remove_fix_versions.is_empty() {
            let ops: Vec<Value> = add_fix_versions.iter().map(|v| serde_json::json!({"add": {"name": v}}))
                .chain(remove_fix_versions.iter().map(|v| serde_json::json!({"remove": {"name": v}})))
                .collect();
            update["fixVersions"] = serde_json::json!(ops);
        }

        let body = serde_json::json!({"fields": fields, "update": update});
        let resp = self.client.put(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&body)
            .send().await?;
        self.check_response(resp).await?;
        Ok(())
    }

    pub async fn assign_issue(&self, key: &str, account_id: Option<&str>) -> Result<()> {
        let url = format!("{}/issue/{}/assignee", self.base_url, key);
        let body = match account_id {
            None => serde_json::json!({"accountId": null}),
            Some(id) => serde_json::json!({"accountId": id}),
        };
        let resp = self.client.put(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&body)
            .send().await?;
        self.check_response(resp).await?;
        Ok(())
    }

    /// Search users assignable to an issue by display name or partial name.
    pub async fn search_assignable_users(&self, issue_key: &str, query: &str) -> Result<Vec<User>> {
        let url = format!("{}/user/assignable/search", self.base_url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("issueKey", issue_key), ("query", query)])
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_link_types(&self) -> Result<Vec<IssueLinkTypeInfo>> {
        let url = format!("{}/issueLinkType", self.base_url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .send().await?;
        let resp = self.check_response(resp).await?;
        let result: IssueLinkTypesResult = resp.json().await?;
        Ok(result.issue_link_types)
    }

    pub async fn create_issue_link(&self, inward_key: &str, outward_key: &str, link_type: &str) -> Result<()> {
        let url = format!("{}/issueLink", self.base_url);
        let body = serde_json::json!({
            "type": { "name": link_type },
            "inwardIssue": { "key": inward_key },
            "outwardIssue": { "key": outward_key },
        });
        let resp = self.client.post(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&body)
            .send().await?;
        self.check_response(resp).await?;
        Ok(())
    }

    /// Find and delete all links between two issues.
    pub async fn delete_issue_link(&self, inward_key: &str, outward_key: &str) -> Result<usize> {
        let find_link_ids = |issue: &Issue, other: &str| -> Vec<String> {
            let other_lower = other.to_lowercase();
            issue.fields.issue_links.iter()
                .filter(|l| {
                    l.inward_issue.as_ref().map(|i| i.key.to_lowercase() == other_lower).unwrap_or(false)
                    || l.outward_issue.as_ref().map(|i| i.key.to_lowercase() == other_lower).unwrap_or(false)
                })
                .map(|l| l.id.clone())
                .collect()
        };

        let issue = self.get_issue(inward_key).await?;
        let mut link_ids = find_link_ids(&issue, outward_key);

        // Fallback: check the other issue in case the link only appears there
        if link_ids.is_empty() {
            let other_issue = self.get_issue(outward_key).await?;
            link_ids = find_link_ids(&other_issue, inward_key);
        }

        if link_ids.is_empty() {
            anyhow::bail!("No link found between {} and {}", inward_key, outward_key);
        }

        for id in &link_ids {
            let url = format!("{}/issueLink/{}", self.base_url, id);
            let resp = self.client.delete(&url)
                .basic_auth(self.auth().0, Some(self.auth().1))
                .send().await?;
            self.check_response(resp).await?;
        }
        Ok(link_ids.len())
    }

    pub async fn add_comment(&self, key: &str, body: &str) -> Result<Comment> {
        let url = format!("{}/issue/{}/comment", self.base_url, key);
        let payload = serde_json::json!({
            "body": {
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": body }]
                }]
            }
        });
        let resp = self.client.post(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .json(&payload)
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_comments(&self, key: &str) -> Result<CommentsField> {
        let url = format!("{}/issue/{}/comment", self.base_url, key);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("orderBy", "created"), ("maxResults", "100")])
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn download_attachment(&self, url: &str) -> Result<Vec<u8>> {
        // Validate URL is on the expected Atlassian instance to prevent SSRF and credential leakage
        let instance_prefix = self.base_url.trim_end_matches("/rest/api/3").to_string() + "/";
        if !url.starts_with(&instance_prefix) {
            return Err(anyhow!("Attachment URL does not match configured Jira instance"));
        }
        let resp = self.client.get(url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }

    pub async fn list_projects(&self, max_results: u32) -> Result<Vec<Project>> {
        let url = format!("{}/project/search", self.base_url);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .query(&[("maxResults", max_results.to_string())])
            .send().await?;
        let resp = self.check_response(resp).await?;
        let result: Value = resp.json().await?;
        let projects: Vec<Project> = serde_json::from_value(result["values"].clone())?;
        Ok(projects)
    }

    pub async fn get_project(&self, key: &str) -> Result<Project> {
        let url = format!("{}/project/{}", self.base_url, key);
        let resp = self.client.get(&url)
            .basic_auth(self.auth().0, Some(self.auth().1))
            .send().await?;
        let resp = self.check_response(resp).await?;
        Ok(resp.json().await?)
    }
}
