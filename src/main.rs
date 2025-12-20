mod structures;

use crate::structures::structs_git::{Asset, Release};
use futures_util::{StreamExt, TryFutureExt};
use reqwest::header::{ACCEPT, AUTHORIZATION, HOST, REFERER, USER_AGENT};
use std::error::Error;

use std::io::{self, BufRead, Cursor, Write};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use shellexpand;
use std::process::{Command, Stdio};
use std::{env, thread};
use tokio::fs;

use reqwest::{Client, blocking as blocking_reqwest};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::runtime::Runtime;
use url::Url;

use anyhow::Result;
use reqwest::blocking::get;
use zip::ZipArchive;

fn extract_output_path(arg_line: &str) -> Option<PathBuf> {
    let parts = shell_words::split(arg_line).ok()?;
    let mut i = 0;
    while i < parts.len() {
        if parts[i] == "-o" {
            if i + 1 < parts.len() {
                let raw = &parts[i + 1];
                let expanded = shellexpand::tilde(raw).into_owned();
                return Some(PathBuf::from(expanded));
            } else {
                return None;
            }
        }
        i += 1;
    }
    None
}

async fn download_curl() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = Path::new("curl.exe");
    if out_path.exists() {
        println!("curl.exe already exists, skipping download.");
        return Ok(());
    }

    let url = "https://curl.se/windows/latest.cgi?p=win64-mingw.zip";
    println!("Downloading {}", url);

    let resp = reqwest::Client::new().get(url).send().await?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()).into());
    }

    let bytes = resp.bytes().await?; // bytes::Bytes
    let reader = Cursor::new(bytes);

    let mut zip = ZipArchive::new(reader)?;

    let target = "curl-8.17.0_5-win64-mingw/bin/curl.exe";
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        if name == target {
            let mut content: Vec<u8> = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut content)?;
            fs::write(out_path, &content).await?;
            println!("Extracted curl.exe");
            return Ok(());
        }
    }

    Err(format!("{} not found in archive", target).into())
}

async fn download_ytdlp(
    app_name: &str,
    github_api: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if fs::metadata(app_name).await.is_ok() {
        return Ok(());
    }

    let client = reqwest::Client::new();
    let mut req = client
        .get(github_api)
        .header(USER_AGENT, "gh-download-rust/0.1")
        .header(ACCEPT, "application/vnd.github.v3+json");
    if let Ok(t) = std::env::var("GITHUB_TOKEN") {
        req = req.header(AUTHORIZATION, format!("token {}", t));
    }
    let releases: Vec<Release> = req.send().await?.error_for_status()?.json().await?;
    let release = releases
        .into_iter()
        .find(|r| !r.draft)
        .ok_or("No release found")?;

    let mut filtered: Vec<Asset> = release
        .assets
        .into_iter()
        .filter(|a| a.name.to_lowercase().ends_with(&app_name.to_lowercase()))
        .collect();
    filtered.sort_by_key(|a| std::cmp::Reverse(a.size.unwrap_or(0)));

    let asset = match filtered.into_iter().next() {
        Some(a) => a,
        None => return Ok(()),
    };

    let url = asset
        .browser_download_url
        .as_deref()
        .ok_or("asset has no browser_download_url")?;

    let resp = client.get(url).send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();

    let mut file = File::create(app_name).await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

pub fn run_and_log(exe: &str, args: &str) -> io::Result<i32> {
    println!("Running command: {} {}", exe, args);
    let args_vec: Vec<String> = if args.trim().is_empty() {
        Vec::new()
    } else {
        shell_words::split(args)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .into_iter()
            .map(|s| shellexpand::tilde(&s).into_owned())
            .collect()
    };

    let mut child = Command::new(exe)
        .args(&args_vec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdout) = child.stdout.take() {
        let mut reader = io::BufReader::new(stdout);
        thread::spawn(move || {
            let mut line = String::new();
            while let Ok(bytes) = reader.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }
                print!("[stdout] {}", line);
                line.clear();
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let mut reader = io::BufReader::new(stderr);
        thread::spawn(move || {
            let mut line = String::new();
            while let Ok(bytes) = reader.read_line(&mut line) {
                if bytes == 0 {
                    break;
                }
                print!("[stderr] {}", line);
                line.clear();
            }
        });
    }

    let status = child.wait()?;
    Ok(status.code().unwrap_or_default())
}

async fn concurrent_download_async(url_img: &str, path: &str) -> Result<()> {
    let url = url_img.replace("\"", "");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    let resp = client
        .get(url)
        .header(USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36")
        .header(REFERER, "https://vk.com/")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP error: {}", resp.status());
    }

    let mut stream = resp.bytes_stream();
    let mut file = File::create(path).await?;

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let b = chunk?;
        file.write_all(&b).await?;
    }

    println!("{}:{}", "Saved image", path);

    Ok(())
}

async fn download_sound(first: &str, second: &str) {
    let first_segment = first.trim_start();
    let second_segment = second.trim_start();

    if first_segment.starts_with("image:") && second_segment.starts_with("yt-dlp:") {
        let image = first_segment["image:".len()..].trim_start();
        let ytdlp = second_segment["yt-dlp:".len()..].trim_start();

        if let Err(e) = run_and_log("yt-dlp.exe", ytdlp) {
            eprintln!("failed to run yt-dlp: {}", e);
        } else {
            if let Some(path) = extract_output_path(ytdlp) {
                if let Some(mut path_str) = path.to_str().map(|s| s.to_string()) {
                    let path_str = path_str.replace(".mp3", ".jpeg"); // или лучше заменить расширение через Path
                    let home: PathBuf = dirs::home_dir().unwrap();

                    // если path_str — абсолютный или относительный путь относительно домашней папки:
                    let file_path = Path::new(&path_str);
                    let full_path = if file_path.is_absolute() {
                        file_path.to_path_buf()
                    } else {
                        home.join(file_path)
                    };
                    let path_str = full_path.to_str().unwrap();

                    concurrent_download_async(image, path_str)
                        .await
                        .expect("TODO: panic message");
                }
            }
        }
    } else {
        eprintln!("Invalid segments for download_sound");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    println!("{}" , "By UnderKo");
    println!("{}" , "https://github.com/underkogit/ytdlp-vk");
    println!("{}" , "Using: ytdlp and curl");

    if let Err(e) = download_ytdlp(
        "yt-dlp.exe",
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases",
    )
    .await
    {
        eprintln!("Downloading failed: {}", e);
    }

    // if let Err(e) = download_curl().await {
    //     eprintln!("Downloading failed: {}", e);
    // }

    loop {
        let mut raw_input = String::new();
        print!("Enter commands: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut raw_input)?;
        let raw_input = raw_input.trim_end();

        if raw_input.starts_with(":help") || raw_input.starts_with(":?") {
            println!(
                "image:\"url\"; yt-dlp -x --audio-format mp3 --embed-thumbnail --add-metadata -o \"PATH/Artist - Title.mp3\" \"URL\""
            );
            continue;
        }

        let segments: Vec<String> = raw_input
            .split(';')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if segments.len() != 2 {
            eprintln!(
                "Error: the string must contain exactly one ';' (example: image:\"url\"; yt-dlp ...)."
            );
            continue;
        }

        let first_segment: &str = segments.get(0).map(|s| s.as_str()).unwrap_or("");
        let second_segment: &str = segments.get(1).map(|s| s.as_str()).unwrap_or("");

        if first_segment.starts_with("image") && second_segment.starts_with("yt-dlp") {
            println!("\"image\": {}", first_segment);
            println!("\"yt-dlp\": {}", second_segment);
            download_sound(first_segment, second_segment).await;
        } else {
            println!(
                "The first argument or the second argument does not match the expected format:\n  first = {}\n  second = {}",
                first_segment, second_segment
            );
            continue;
        }
    }
}
