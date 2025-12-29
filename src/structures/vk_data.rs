use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Data {
    #[serde(default)]
    pub safeArtist: String,
    #[serde(default)]
    pub safeTitle: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub index: String,
}

#[derive(Serialize)]
pub struct Demo {
    pub safeArtistTitle: String,
    pub Uri: String,
}
