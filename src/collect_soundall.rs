use crate::structures::vk_data::{Data, Demo};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use urlencoding::encode;

fn try_load(path: &Path) -> Option<Data> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Data>(&s).ok())
}

pub fn collect_sb(directory: &str) -> i32 {
    let dir = Path::new(directory);
    if !dir.exists() || !dir.is_dir() {
        eprintln!("Указанная директория не существует.");
        return 1;
    }

    let re_nonword = Regex::new(r"[^\w]+").unwrap();
    let re_digits = Regex::new(r"[0-9]+").unwrap();

    let entries: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => {
            eprintln!("Ошибка чтения директории.");
            return 2;
        }
    };

    let mut results: Vec<Demo> = entries
        .par_iter()
        .filter_map(|subdir| {
            let file = subdir.join("data.json");
            if !file.exists() {
                return None;
            }
            let m = try_load(&file)?;
            let artist = m.safeArtist.trim().to_string();
            let title = m.safeTitle.trim().to_string();
            let index = m.index.trim().to_string();
            if artist.is_empty() && title.is_empty() {
                return None;
            }
            let combined = format!("{} - {}", artist, title);
            let cleaned = re_nonword.replace_all(&combined, "").to_string();
            let cleaned = re_digits.replace_all(&cleaned, "").to_string();
            if cleaned.is_empty() || index.is_empty() {
                return None;
            }
            Some(Demo {
                safeArtistTitle: combined.clone(),
                Uri: format!("https://vk.ru/audio?q={}", encode(&combined)),
            })
        })
        .collect();

    results.sort_by(|a, b| {
        a.safeArtistTitle
            .to_lowercase()
            .cmp(&b.safeArtistTitle.to_lowercase())
    });
    results.dedup_by(|a, b| a.safeArtistTitle == b.safeArtistTitle && a.Uri == b.Uri);

    let out_path = dir.join("soundall.json");
    match to_string_pretty(&results) {
        Ok(json) => {
            if let Err(_) = fs::write(&out_path, json) {
                eprintln!("Error writing the file.");
                return 2;
            }
            println!(
                "Done. Saved {} records in {}",
                results.len(),
                out_path.display()
            );
            0
        }
        Err(_) => {
            eprintln!("Serialization error.");
            2
        }
    }
}
