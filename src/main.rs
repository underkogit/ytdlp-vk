use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, Responder, post, web};
use anyhow::Result;
use std::io::{self, Write};
use std::path::Path;
use tokio::task::JoinHandle;

mod download_manager;
mod path_ext;
mod process_manager;
mod structures;
mod zip_extractor;

use crate::download_manager::{
    check_bin_contains_ffmpeg_and_ytdlp, fetch_ffmpeg_release_async, fetch_ytdlp_release_async,
    handle_sound_command_async,
};

#[post("/download")]
async fn download(body: String) -> impl Responder {
    println!("HTTP received: {}", body);

    handle_sound_command_async(body.as_str()).await;
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(format!("Received {} bytes", body.len()))
}

async fn init_console() {
    println!("By UnderKo");
    println!("https://github.com/underkogit/ytdlp-vk");
    println!("Using: ytdlp, ffmpeg");

    if (!check_bin_contains_ffmpeg_and_ytdlp(Path::new("bin_"))) {
        if let Err(e) = fetch_ffmpeg_release_async(
            "ffmpeg-master-latest-win64-lgpl-shared.zip",
            "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases",
        )
        .await
        {
            eprintln!("FFmpeg download failed: {}", e);
        }

        if let Err(e) = fetch_ytdlp_release_async(
            "yt-dlp.exe",
            "https://api.github.com/repos/yt-dlp/yt-dlp/releases",
        )
        .await
        {
            eprintln!("yt-dlp download failed: {}", e);
        }
    }
}

fn cors_middleware() -> Cors {
    Cors::default()
        .allowed_methods(vec!["GET", "POST", "DELETE", "PUT"])
        .allowed_headers(vec![
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
            http::header::CONTENT_TYPE,
        ])
        .max_age(3600)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_console().await;

    // Создаём и запускаем сервер
    let server = HttpServer::new(|| {
        App::new()
            .wrap(cors_middleware())
            // permissive удобно для разработки; при проде лучше настроить конкретные origin
            .wrap(Cors::permissive())
            .service(download)
    })
    .bind(("127.0.0.1", 1488))?
    .run();

    // Получаем handle и запускаем сервер в фоне
    let handle = server.handle();
    let server_task: JoinHandle<_> = tokio::spawn(server);

    // Простой REPL для обработки команд из консоли
    loop {
        let mut raw_input = String::new();
        print!("Enter commands: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut raw_input)?;
        let raw_input = raw_input.trim_end();

        handle_sound_command_async(raw_input).await;

        if raw_input.starts_with(":help") || raw_input.starts_with(":?") {
            println!(
                "image:\"url\"; yt-dlp -x --audio-format mp3 --embed-thumbnail --add-metadata -o \"PATH/Artist - Title.mp3\" \"URL\""
            );
            continue;
        }

        if raw_input.eq_ignore_ascii_case("quit") || raw_input.eq_ignore_ascii_case("exit") {
            // Останавливаем сервер и ждём задачи
            let _ = handle.stop(true).await;
            let _ = server_task.await;
            break;
        }
    }

    Ok(())
}
