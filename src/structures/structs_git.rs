use serde::Deserialize;
use chrono::{DateTime, Utc};


#[derive(Debug, Deserialize)]
pub struct User {
    pub login: String,
    pub id: u64,
    pub node_id: Option<String>,
    pub avatar_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub url: String,
    pub id: u64,
    pub name: String,
    pub label: Option<String>,
    pub content_type: Option<String>,
    pub state: Option<String>,
    pub size: Option<u64>,
    pub download_count: Option<u64>,
    pub browser_download_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct Release {
    pub url: Option<String>,
    pub html_url: Option<String>,
    pub assets_url: Option<String>,
    pub upload_url: Option<String>,
    pub tarball_url: Option<String>,
    pub zipball_url: Option<String>,
    pub id: Option<u64>,
    pub node_id: Option<String>,
    pub tag_name: Option<String>,
    pub target_commitish: Option<String>,
    pub name: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub created_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub author: Option<User>,
    pub assets: Vec<Asset>,
    pub discussion_url: Option<String>,
}