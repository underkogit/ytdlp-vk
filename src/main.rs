use actix_web::dev::Service;
mod structures;
mod zip_extractor;

use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};

use crate::structures::structs_git::{Asset, Release};
use futures_util::{StreamExt, TryFutureExt};
use reqwest::header::{ACCEPT, AUTHORIZATION, HOST, REFERER, USER_AGENT};
use std::error::Error;

use std::io::{self, BufRead, Cursor, Read, Write};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use shellexpand;
use std::process::{Command, Stdio};
use std::{env, thread};
use tokio::fs;

use actix_web::http::header;
use reqwest::{Client, blocking as blocking_reqwest};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::runtime::Runtime;
use url::Url;

use crate::zip_extractor::extract_prefix_from_zip;
use anyhow::Result;
use reqwest::blocking::get;
use tokio::sync::oneshot;
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
fn join_path(folder: &str, filename: &str) -> PathBuf {
    let mut path = PathBuf::from(folder);
    path.push(filename);
    path
}
async fn download_ytdlp_async(
    app_name: &str,
    github_api: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if fs::metadata(app_name).await.is_ok() {
        return Ok(());
    }
    println!("downloading ytdlp for {}", app_name);
    println!("{}", github_api);
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

    let mut file = File::create(join_path("bin_", app_name)).await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    Ok(())
}

async fn download_ffmpeg_async(
    app_name: &str,
    github_api: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if fs::metadata(app_name).await.is_ok() {
        return Ok(());
    }
    println!("downloading ffmpeg for {}", app_name);
    println!("{}", github_api);
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

    let zip_path = Path::new("ffmpeg-master-latest-win64-lgpl-shared.zip");
    let dest = Path::new("bin_");
    let target_prefix = "ffmpeg-master-latest-win64-lgpl-shared/bin/";

    extract_prefix_from_zip(zip_path, dest, target_prefix)
        .map_err(|e| format!("Ошибка при распаковке: {}", e))?;

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

async fn baner_download_async(url_img: &str, path: &str) -> Result<String> {
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

    Ok(String::from(path))
}

fn run_ffmpeg_and_capture(stderr_out: &mut String, mut cmd: Command) -> io::Result<std::process::ExitStatus> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(mut s) = child.stderr.take() {
        s.read_to_string(stderr_out)?;
    }
    child.wait()
}

fn set_title_with_ffmpeg(
    ffmpeg_path: &str,
    input: &str,
    output: &str,
    title: &str,
    banner_path: String,
) -> io::Result<()> {
    // Попытка 1: быстрый stream copy
    let mut stderr = String::new();
    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-y")
        .arg("-i").arg(input)
        .arg("-i").arg(&banner_path)
        .arg("-map").arg("0:a?")
        .arg("-map").arg("1:v?")
        .arg("-metadata").arg(format!("title={}", title))
        .arg("-metadata").arg(format!("description={}", title))
        .arg("-disposition:v:0").arg("attached_pic")
        .arg("-c").arg("copy")
        .arg(output);
    let status = run_ffmpeg_and_capture(&mut stderr, cmd)?;
    if status.success() {
        return Ok(());
    }

    // Если copy не сработал — повторный запуск с явным id3v2 и перекодированием в mp3
    stderr.clear();
    let mut cmd2 = Command::new(ffmpeg_path);
    cmd2.arg("-y")
        .arg("-i").arg(input)
        .arg("-i").arg(&banner_path)
        .arg("-map").arg("0:a?")
        .arg("-map").arg("1:v?")
        .arg("-metadata").arg(format!("title={}", title))
        .arg("-metadata").arg(format!("description={}", title))
        // Пометить картинку как attached pic
        .arg("-disposition:v:0").arg("attached_pic")
        // Указать id3v2 версию и явно кодировать аудио в mp3
        .arg("-id3v2_version").arg("3")
        .arg("-c:a").arg("libmp3lame")
        .arg("-b:a").arg("192k")
        // для видео/изображения — mp3 теги, поэтому копируем/встраиваем картинку как attached_pic
        .arg(output);
    let status2 = run_ffmpeg_and_capture(&mut stderr, cmd2)?;
    if status2.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("ffmpeg failed (both attempts):\nFirst attempt stderr:\n{}\nSecond attempt stderr:\n{}", stderr, stderr),
        ))
    }
}

async fn remove_and_rename(original_path: &Path, out_path_o_str: &str) -> io::Result<()> {
    // 1) удалить файл original_path, если он существует
    if original_path.exists() {
        fs::remove_file(original_path).await?;
    }

    // 2) сформировать новый путь, заменив ".mp3_t" в имени файла (только последний сегмент)
    let out_path = Path::new(out_path_o_str);
    let parent = out_path.parent().unwrap_or_else(|| Path::new(""));
    let file_name = out_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid out_path_o_str filename",
            )
        })?;

    let new_file_name = file_name.replace(".mp3_t", "");

    let new_path = parent.join(new_file_name);

    // 3) переименовать
    fs::rename(out_path, &new_path).await?;

    Ok(())
}

async fn download_sound_async(first: &str, second: &str) -> Result<(), io::Error> {
    let (image, ytdlp) = {
        let a = first.trim_start();
        let b = second.trim_start();
        if a.starts_with("image:") && b.starts_with("yt-dlp:") {
            (
                a["image:".len()..].trim_start(),
                b["yt-dlp:".len()..].trim_start(),
            )
        } else {
            eprintln!("Invalid segments for download_sound");
            return Ok(());
        }
    };

    if let Err(e) = run_and_log("bin_/yt-dlp.exe", ytdlp) {
        eprintln!("failed to run yt-dlp: {}", e);
        return Ok(());
    }
    let out_path = match extract_output_path(ytdlp).and_then(|p| p.to_str().map(|s| s.to_string()))
    {
        Some(p) => p,
        None => {
            eprintln!("Could not determine output path");
            return Ok(());
        }
    };

    let img_path = {
        let p = if out_path.ends_with(".mp3") {
            out_path.trim_end_matches(".mp3").to_string() + ".jpeg"
        } else {
            out_path.clone() + ".jpeg"
        };
        let p = Path::new(&p);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            dirs::home_dir()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home dir"))?
                .join(p)
        }
    };

    let full_path_image = baner_download_async(
        image,
        img_path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid path"))?,
    )
    .await
    .map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("banner download failed: {}", e),
        )
    })?;

    let tmp = format!("{}_t.mp3", out_path);
    let file_name = Path::new(&out_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid filename"))?;

    set_title_with_ffmpeg(
        r"bin_\ffmpeg.exe",
        &out_path,
        &tmp,
        file_name,
        full_path_image,
    )
    .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ffmpeg failed: {}", e)))?;

    let _ = remove_and_rename(out_path.as_ref(), tmp.as_ref()).await;
    Ok(())
}
// async fn download_sound_async(first: &str, second: &str) {
//     let first_segment = first.trim_start();
//     let second_segment = second.trim_start();
//
//     if first_segment.starts_with("image:") && second_segment.starts_with("yt-dlp:") {
//         let image = first_segment["image:".len()..].trim_start();
//         let ytdlp = second_segment["yt-dlp:".len()..].trim_start();
//
//         if let Err(e) = run_and_log("bin_/yt-dlp.exe", ytdlp) {
//             eprintln!("failed to run yt-dlp: {}", e);
//         } else {
//             if let Some(path) = extract_output_path(ytdlp) {
//                 if let Some(mut original_path) = path.to_str().map(|s| s.to_string()) {
//                     let image_path = original_path.replace(".mp3", ".jpeg");
//                     let home: PathBuf = dirs::home_dir().unwrap();
//
//                     let file_path = Path::new(&image_path);
//                     let full_path = if file_path.is_absolute() {
//                         file_path.to_path_buf()
//                     } else {
//                         home.join(file_path)
//                     };
//                     let path_str = full_path.to_str().unwrap();
//
//                     baner_download_async(image, path_str)
//                         .await
//                         .expect("TODO: panic message");
//                     let out_path_o_str = original_path.clone() + "_t.mp3";
//
//                     let file_name = Path::new(&original_path) .file_name()
//                         .and_then(|s| s.to_str())
//                         .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid out_path_o_str filename"));
//
//                     set_title_with_ffmpeg(
//                         r"bin_\ffmpeg.exe",
//                         &*original_path,
//                         &*out_path_o_str,
//                         file_name.expect("REASON"),
//                     )
//                     .expect("TODO: panic message");
//
//                     let _ =
//                         remove_and_rename(original_path.as_ref(), out_path_o_str.as_ref()).await;
//                 }
//             }
//         }
//     } else {
//         eprintln!("Invalid segments for download_sound");
//     }
// }

#[post("/download")]
async fn download(body: String) -> impl Responder {
    println!("HTTP received: {}", body);

    download_sound_a(body.as_str()).await;

    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(format!("Received {} bytes", body.len()))
}

async fn init_console() {
    println!("{}", "By UnderKo");
    println!("{}", "https://github.com/underkogit/ytdlp-vk");
    println!("{}", "Using: ytdlp and curl");

    if let Err(e) = download_ffmpeg_async(
        "ffmpeg-master-latest-win64-lgpl-shared.zip",
        "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases",
    )
    .await
    {
        eprintln!("Downloading failed: {}", e);
    }

    if let Err(e) = download_ytdlp_async(
        "yt-dlp.exe",
        "https://api.github.com/repos/yt-dlp/yt-dlp/releases",
    )
    .await
    {
        eprintln!("Downloading failed: {}", e);
    }
}

async fn download_sound_a(raw_input: &str) {
    let segments: Vec<String> = raw_input
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.len() != 2 {
        eprintln!(
            "Error: the string must contain exactly one ';' (example: image:\"url\"; yt-dlp ...)."
        );
        return;
    }

    let first_segment: &str = segments.get(0).map(|s| s.as_str()).unwrap_or("");
    let second_segment: &str = segments.get(1).map(|s| s.as_str()).unwrap_or("");

    if first_segment.starts_with("image") && second_segment.starts_with("yt-dlp") {
        println!("\"image\": {}", first_segment);
        println!("\"yt-dlp\": {}", second_segment);
        download_sound_async(first_segment, second_segment).await;
    } else {
        println!(
            "The first argument or the second argument does not match the expected format:\n  first = {}\n  second = {}",
            first_segment, second_segment
        );
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_console().await;

    let server = HttpServer::new(|| {
        // Настройте origin под ваш фронтенд (или .send_wildcard() для разработки)
        let cors = Cors::default()
            .allowed_methods(vec!["GET", "POST", "DELETE", "PUT"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::ACCEPT,
                http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(Cors::permissive())
            .service(download)
    })
    .bind(("127.0.0.1", 8080))? // привязка
    .run();

    let handle = server.handle();
    let server_task = tokio::spawn(async move {
        let _ = server.await;
    });

    loop {
        let mut raw_input = String::new();
        print!("Enter commands: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut raw_input)?;
        let raw_input = raw_input.trim_end();

        download_sound_a(raw_input).await;

        if raw_input.starts_with(":help") || raw_input.starts_with(":?") {
            println!(
                "image:\"url\"; yt-dlp -x --audio-format mp3 --embed-thumbnail --add-metadata -o \"PATH/Artist - Title.mp3\" \"URL\""
            );
            continue;
        }

        if raw_input.eq_ignore_ascii_case("quit") || raw_input.eq_ignore_ascii_case("exit") {
            let _ = handle.stop(true).await;
            let _ = server_task.await;
            break;
        }
    }

    Ok(())
}
