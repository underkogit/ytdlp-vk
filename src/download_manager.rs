use crate::path_ext::{extract_output_path, join_path, remove_and_rename};
use crate::process_manager::{embed_title_and_artwork_with_ffmpeg, spawn_and_log_io};
use crate::structures::structs_git::{Asset, Release};
use crate::zip_extractor::extract_prefix_from_zip;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, REFERER, USER_AGENT};
use std::error::Error;
use std::io;
use std::io::Cursor;
use std::path::Path;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

/// Асинхронно проверяет наличие локального файла "curl.exe" и при его отсутствии:
/// - скачивает ZIP с https://curl.se/windows/latest.cgi?p=win64-mingw.zip;
/// - распаковывает из архива файл "curl-8.17.0_5-win64-mingw/bin/curl.exe";
/// - сохраняет его как "curl.exe".
/// Возвращает ошибку при неудачном HTTP‑ответе, отсутствии целевого файла в архиве или при ошибках ввода/вывода.
pub async fn fetch_curl_exe() -> anyhow::Result<(), Box<dyn std::error::Error>> {
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

/// Асинхронно загружает релиз yt-dlp с GitHub для указанного app_name:
/// - если файл с именем app_name уже существует — ничего не делает;
/// - запрашивает список релизов через GitHub API (поддерживается GITHUB_TOKEN для авторизации);
/// - выбирает первый не‑draft релиз, фильтрует assets по совпадению имени с app_name и сортирует по размеру;
/// - скачивает выбранный asset потоково и сохраняет под именем "bin_\\<app_name>".
/// Возвращает сетевые и файловые ошибки при неудаче.
pub async fn fetch_ytdlp_release_async(
    app_name: &str,
    github_api: &str,
) -> anyhow::Result<(), Box<dyn Error + Send + Sync>> {
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

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        req = req.header(AUTHORIZATION, format!("token {}", token));
    }

    let releases: Vec<Release> = req.send().await?.error_for_status()?.json().await?;
    let release = releases
        .into_iter()
        .find(|r| !r.draft)
        .ok_or("No release found")?;

    let mut assets: Vec<Asset> = release
        .assets
        .into_iter()
        .filter(|a| a.name.to_lowercase().ends_with(&app_name.to_lowercase()))
        .collect();
    assets.sort_by_key(|a| std::cmp::Reverse(a.size.unwrap_or(0)));
    let asset = match assets.into_iter().next() {
        Some(a) => a,
        None => return Ok(()),
    };

    let url = asset
        .browser_download_url
        .as_deref()
        .ok_or("asset has no browser_download_url")?;
    let resp = client.get(url).send().await?.error_for_status()?;

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut file = File::create(join_path("bin_", app_name)).await?;
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }
    file.flush().await?;
    Ok(())
}

/// Асинхронно загружает релиз ffmpeg с GitHub для указанного app_name:
/// - если файл с именем app_name уже существует — пропускает работу;
/// - поведение загрузки аналогично fetch_ytdlp_release_async (GitHub API, фильтрация assets);
/// - скачивает выбранный asset потоково и сохраняет в файл app_name;
/// - затем распаковывает из ожидаемого ZIP "ffmpeg-master-latest-win64-lgpl-shared.zip"
///   все файлы с префиксом "ffmpeg-master-latest-win64-lgpl-shared/bin/" в каталог "bin_".
/// Возвращает ошибки при сетевых, файловых или распаковочных сбоях.
pub async fn fetch_ffmpeg_release_async(
    app_name: &str,
    github_api: &str,
) -> anyhow::Result<(), Box<dyn Error + Send + Sync>> {
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

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        req = req.header(AUTHORIZATION, format!("token {}", token));
    }

    let releases: Vec<Release> = req.send().await?.error_for_status()?.json().await?;
    let release = releases
        .into_iter()
        .find(|r| !r.draft)
        .ok_or("No release found")?;

    let mut assets: Vec<Asset> = release
        .assets
        .into_iter()
        .filter(|a| a.name.to_lowercase().ends_with(&app_name.to_lowercase()))
        .collect();
    assets.sort_by_key(|a| std::cmp::Reverse(a.size.unwrap_or(0)));
    let asset = match assets.into_iter().next() {
        Some(a) => a,
        None => return Ok(()),
    };

    let url = asset
        .browser_download_url
        .as_deref()
        .ok_or("asset has no browser_download_url")?;
    let resp = client.get(url).send().await?.error_for_status()?;
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut file = File::create(app_name).await?;
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?).await?;
    }
    file.flush().await?;

    let zip_path = Path::new("ffmpeg-master-latest-win64-lgpl-shared.zip");
    let dest = Path::new("bin_");
    let target_prefix = "ffmpeg-master-latest-win64-lgpl-shared/bin/";
    extract_prefix_from_zip(zip_path, dest, target_prefix)
        .map_err(|e| format!("Ошибка при распаковке: {}", e))?;

    Ok(())
}

/// Асинхронно обрабатывает строку из двух сегментов: первый должен начинаться с "image:", второй — с "yt-dlp:".
/// - извлекает URL изображения и команду yt-dlp;
/// - запускает yt-dlp (run_and_log) для скачивания аудиофайла;
/// - определяет путь к выходному файлу, формирует путь к изображению (в том числе относительный -> домашняя папка);
/// - скачивает баннер (download_banner_image_async), затем вызывает set_title_with_ffmpeg для установки обложки/тегов;
/// - переименовывает временный файл в итоговый.
/// При ошибках печатает сообщения и корректно возвращает/обрабатывает ошибки ввода-вывода.
pub async fn process_and_tag_sound_async(
    first: &str,
    second: &str,
) -> anyhow::Result<(), std::io::Error> {
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

    if let Err(e) = spawn_and_log_io("bin_/yt-dlp.exe", ytdlp) {
        eprintln!("failed to run yt-dlp: {}", e);
        return Ok(());
    }

    let out_path =
        match extract_output_path(ytdlp).and_then(|p| p.to_str().map(ToString::to_string)) {
            Some(p) => p,
            None => {
                eprintln!("Could not determine output path");
                return Ok(());
            }
        };

    let img_path = {
        let base = if out_path.ends_with(".mp3") {
            out_path.trim_end_matches(".mp3").to_owned() + ".jpeg"
        } else {
            out_path.clone() + ".jpeg"
        };
        let p = Path::new(&base);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            dirs::home_dir()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "home dir"))?
                .join(p)
        }
    };

    let full_path_image = download_banner_image_async(
        image,
        img_path
            .to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid path"))?,
    )
    .await
    .map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("banner download failed: {}", e),
        )
    })?;

    let tmp = format!("{}_t.mp3", out_path);
    let file_name = Path::new(&out_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid filename"))?;

    embed_title_and_artwork_with_ffmpeg(
        r"bin_\ffmpeg.exe",
        &out_path,
        &tmp,
        file_name,
        full_path_image,
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("ffmpeg failed: {}", e)))?;

    let _ = remove_and_rename(out_path.as_ref(), tmp.as_ref()).await;
    Ok(())
}

/// Асинхронно скачивает изображение по заданному URL в указанный путь:
/// - убирает кавычки вокруг URL, делает GET запрос с заголовками User-Agent и Referer;
/// - проверяет HTTP-статус, потоково записывает содержимое в файл;
/// - возвращает строку с путём к сохранённому файлу или ошибку при сбое.
pub async fn download_banner_image_async(url_img: &str, path: &str) -> anyhow::Result<String> {
    let url = url_img.trim_matches('"');

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .build()?;

    let resp = client
        .get(url)
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36",
        )
        .header(REFERER, "https://vk.com/")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("HTTP error: {}", resp.status());
    }

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(path).await?;

    while let Some(chunk) = stream.next().await {
        let b = chunk?;
        file.write_all(&b).await?;
    }

    println!("Saved image: {}", path);

    Ok(path.to_string())
}

/// Обрабатывает входную строку командой вида: image:"url"; yt-dlp ...
/// - делит строку по ';', ожидает ровно два непустых сегмента;
/// - если форматы сегментов верны, печатает их и вызывает process_and_tag_sound_async;
/// - в противном случае печатает сообщение об ошибке формата.
pub async fn handle_sound_command_async(raw_input: &str) {
    let mut segments: Vec<&str> = raw_input
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if segments.len() != 2 {
        eprintln!(
            "Error: the string must contain exactly one ';' (example: image:\"url\"; yt-dlp ...)."
        );
        return;
    }

    let first_segment = segments.remove(0);
    let second_segment = segments.remove(0);

    if first_segment.starts_with("image") && second_segment.starts_with("yt-dlp") {
        println!("\"image\": {}", first_segment);
        println!("\"yt-dlp\": {}", second_segment);
        process_and_tag_sound_async(first_segment, second_segment).await;
    } else {
        println!(
            "The first argument or the second argument does not match the expected format:\n  first = {}\n  second = {}",
            first_segment, second_segment
        );
    }
}
