use std::io::{self, BufRead, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

/// Запускает внешний процесс с заданным исполняемым файлом и строкой аргументов.
/// Логирует stdout и stderr в реальном времени (метки "[stdout]" / "[stderr]") в отдельных потоках,
/// ожидает завершения процесса и возвращает код выхода (i32).
pub fn spawn_and_log_io(exe: &str, args: &str) -> io::Result<i32> {
    println!("Running command: {} {}", exe, args);
    let args_vec = if args.trim().is_empty() {
        Vec::new()
    } else {
        shell_words::split(args)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .into_iter()
            .map(|s| shellexpand::tilde(&s).into_owned())
            .collect::<Vec<_>>()
    };

    let mut child = Command::new(exe)
        .args(&args_vec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(out) = child.stdout.take() {
        thread::spawn(|| {
            let mut r = io::BufReader::new(out);
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) != 0 {
                print!("[stdout] {}", line);
                line.clear();
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        thread::spawn(|| {
            let mut r = io::BufReader::new(err);
            let mut line = String::new();
            while r.read_line(&mut line).unwrap_or(0) != 0 {
                print!("[stderr] {}", line);
                line.clear();
            }
        });
    }

    let status = child.wait()?;
    Ok(status.code().unwrap_or_default())
}

/// Принимает готовую Command, перенаправляет stdout в null и stderr — в буфер.
/// Запускает процесс, считывает весь stderr в переданную String и возвращает ExitStatus.
fn capture_stderr_for_command(stderr_out: &mut String, mut cmd: Command) -> io::Result<ExitStatus> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let Some(mut s) = child.stderr.take() {
        s.read_to_string(stderr_out)?;
    }
    child.wait()
}

/// Использует ffmpeg для встраивания метаданных title/description и обложки (banner).
/// Сначала пытается быстрый stream-copy; если неудачно — повторно запускает ffmpeg с принудительным
/// выставлением id3v2 и перекодированием аудио в mp3.
pub fn embed_title_and_artwork_with_ffmpeg(
    ffmpeg_path: &str,
    input: &str,
    output: &str,
    title: &str,
    banner_path: String,
) -> io::Result<()> {
    let mut stderr = String::new();

    let base = |mut c: Command| {
        c.arg("-y")
            .arg("-i")
            .arg(input)
            .arg("-i")
            .arg(&banner_path)
            .arg("-map")
            .arg("0:a?")
            .arg("-map")
            .arg("1:v?")
            .arg("-metadata")
            .arg(format!("title={}", title))
            .arg("-metadata")
            .arg(format!("description={}", title))
            .arg("-disposition:v:0")
            .arg("attached_pic");
        c
    };

    // attempt 1: stream copy
    let mut cmd = base(Command::new(ffmpeg_path));
    cmd.arg("-c").arg("copy").arg(output);
    if capture_stderr_for_command(&mut stderr, cmd)?.success() {
        return Ok(());
    }

    // attempt 2: force id3v2 and re-encode audio to mp3
    stderr.clear();
    let mut cmd2 = base(Command::new(ffmpeg_path));
    cmd2.arg("-id3v2_version")
        .arg("3")
        .arg("-c:a")
        .arg("libmp3lame")
        .arg("-b:a")
        .arg("192k")
        .arg(output);
    let status2 = capture_stderr_for_command(&mut stderr, cmd2)?;
    if status2.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "ffmpeg failed (both attempts). Combined stderr:\n{}",
                stderr
            ),
        ))
    }
}
