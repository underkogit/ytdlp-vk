mod structures;

use crate::structures::structs_git::{Asset, Release};
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use std::error::Error;

use std::io::{self, BufRead, Write};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use std::process::{Command, Stdio};
use std::thread;

async fn download_ytdlp(
    app_name: &str,
    github_api: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let mut req = client
        .get(github_api)
        .header(USER_AGENT, "gh-download-rust/0.1")
        .header(ACCEPT, "application/vnd.github.v3+json");
    let token = std::env::var("GITHUB_TOKEN").ok();
    if let Some(t) = token {
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
        .filter(|a| a.name.to_lowercase().ends_with(app_name))
        .collect();
    filtered.sort_by_key(|a| std::cmp::Reverse(a.size.unwrap_or(0)));

    if filtered.is_empty() {
        return Ok(());
    }

    let asset = &filtered[0];
    let url = asset
        .browser_download_url
        .as_deref()
        .ok_or("asset has no browser_download_url")?;

    let resp = client.get(url).send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();

    let mut file = File::create(app_name).await?; // async file
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    Ok(())
}

pub fn run_and_log(exe: &str, args: &str) -> io::Result<i32> {
    let args_vec: Vec<&str> = if args.trim().is_empty() {
        Vec::new()
    } else {
        args.split_whitespace().collect()
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

fn download_sound(first: &str, second: &str) -> u8 {
    let first_segment = first.trim_start();
    let second_segment = second.trim_start();

    if (first_segment.contains("image:") && second_segment.contains("yt-dlp:")) {
        let mut image = &first_segment["image:".len()..];
        image = image.strip_prefix(' ').unwrap_or(image);

        let mut ytdlp = &second_segment["yt-dlp:".len()..];
        ytdlp = ytdlp.strip_prefix(' ').unwrap_or(ytdlp);

        run_and_log("yt-dlp.exe", ytdlp).expect("failed to run my_program");
    }

    //run_and_log("", _second_segment).expect("failed to run my_program");

    0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Err(e) = download_ytdlp(
        "yt-dlp.exe",
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases",
    )
    .await
    {
        eprintln!("Downloading failed: {}", e);
    }

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

        // Разбиваем по ';', убираем пустые части и пробелы вокруг
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

            download_sound(first_segment, second_segment);
        } else {
            println!(
                "The first argument or the second argument does not match the expected format:\n  first = {}\n  second = {}",
                first_segment, second_segment
            );
            continue;
        }
    }
}
