use serde::Deserialize;
use std::{fmt, fs, path::PathBuf};

#[derive(Deserialize, Clone)]
pub struct Config {
    pub cloud_id: String,
    pub email: String,
    #[serde(default)]
    pub jira_api_token: String,
    /// API token for Confluence. Falls back to jira_api_token if unset. CONFLUENCE_API_TOKEN env var overrides.
    #[serde(default)]
    pub confluence_api_token: Option<String>,
    /// Web base URL for opening issues/pages in the browser, e.g. https://mycompany.atlassian.net
    pub server: Option<String>,
    pub default_project: Option<String>,
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("cloud_id", &self.cloud_id)
            .field("email", &self.email)
            .field("jira_api_token", &"[REDACTED]")
            .field("confluence_api_token", &self.confluence_api_token.as_ref().map(|_| "[REDACTED]"))
            .field("server", &self.server)
            .field("default_project", &self.default_project)
            .finish()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CustomField {
    pub name: String,
    pub id: String,
    #[serde(default)]
    pub lines: Option<usize>,
    #[serde(default)]
    pub markdown: bool,
}

impl Config {
    pub fn api_base_url(&self) -> String {
        format!("https://api.atlassian.com/ex/jira/{}/rest/api/3", self.cloud_id)
    }

    pub fn confluence_api_base_url(&self) -> String {
        format!("https://api.atlassian.com/ex/confluence/{}/wiki/rest/api", self.cloud_id)
    }

    pub fn browse_url(&self, key: &str) -> Option<String> {
        let base = std::env::var("JIRA_BROWSER_SERVER")
            .ok()
            .or_else(|| self.server.clone())?;
        Some(format!("{}/browse/{}", base.trim_end_matches('/'), key))
    }

    pub fn confluence_browse_url(&self, space_key: &str, page_id: &str) -> Option<String> {
        let base = std::env::var("CONFLUENCE_BROWSER_SERVER")
            .ok()
            .or_else(|| self.server.clone())?;
        Some(format!(
            "{}/wiki/spaces/{}/pages/{}",
            base.trim_end_matches('/'),
            space_key,
            page_id
        ))
    }

    pub fn config_path() -> PathBuf {
        let xdg_path = dirs::home_dir()
            .map(|h| h.join(".config").join("atlassian-cli").join("config.toml"));
        if let Some(p) = xdg_path {
            if p.exists() { return p; }
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("atlassian-cli")
            .join("config.toml")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::config_path();
        Self::from_file(&path)
    }

    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                let mode = meta.permissions().mode();
                if (mode & 0o077) != 0 {
                    eprintln!(
                        "Warning: config file {} has permissions {:04o}. Run: chmod 600 {}",
                        path.display(), mode & 0o777, path.display()
                    );
                }
            }
        }
        let contents = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Cannot read config at {}: {}", path.display(), e))?;
        let mut config: Config = toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("Invalid config: {}", e))?;
        if let Ok(token) = std::env::var("JIRA_API_TOKEN") {
            config.jira_api_token = token;
        }
        if let Ok(token) = std::env::var("CONFLUENCE_API_TOKEN") {
            config.confluence_api_token = Some(token);
        }
        if config.jira_api_token.is_empty() {
            return Err(anyhow::anyhow!(
                "jira_api_token not set in config and JIRA_API_TOKEN env var is missing"
            ));
        }
        Ok(config)
    }
}
