use std::io;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Собирает путь из папки и имени файла.
///
/// # Аргументы
/// - `folder`: путь к папке (абсолютный или относительный).
/// - `filename`: имя файла или относительный путь внутри `folder`.
///
/// # Возвращает
/// `PathBuf` — результат эквивалентный `Path::new(folder).join(filename)`.
pub fn join_path(folder: &str, filename: &str) -> PathBuf {
    Path::new(folder).join(filename)
}

/// Извлекает путь, указанный после ключа `-o` в строке аргументов.
///
/// Поддерживает кавычки и экранирование через `shell_words`, а также развёртывание `~`
/// через `shellexpand`.
///
/// # Аргументы
/// - `arg_line`: строка аргументов (например, командная строка).
///
/// # Возвращает
/// `Option<PathBuf>`:
/// - `Some(PathBuf)` с развернутым путем, если найден ключ `-o` с последующим аргументом;
/// - `None` если парсинг не удался или `-o` не найден.
pub fn extract_output_path(arg_line: &str) -> Option<PathBuf> {
    shell_words::split(arg_line)
        .ok()?
        .windows(2)
        .find(|w| w[0] == "-o")
        .map(|w| shellexpand::tilde(&w[1]).into_owned().into())
}

/// Удаляет файл по `original_path`, если он существует, затем переименовывает файл,
/// указанной строкой `out_path_o_str`, убирая из имени суффикс `.mp3_t` (только в последней части пути).
///
/// # Аргументы
/// - `original_path`: путь к возможному файлу, который надо удалить перед переименованием.
/// - `out_path_o_str`: строка пути к текущему файлу, который нужно переименовать.
///
/// # Ошибки
/// Возвращает `io::Error` в случае проблем с удалением/переименованием или если
/// `out_path_o_str` не содержит корректного имени файла.
pub async fn remove_and_rename(original_path: &Path, out_path_o_str: &str) -> io::Result<()> {
    // удалить original_path, если существует
    if original_path.exists() {
        fs::remove_file(original_path).await?;
    }

    // сформировать новый путь, заменив ".mp3_t" в имени файла (только последний сегмент)
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

    // переименовать
    fs::rename(out_path, &new_path).await?;

    Ok(())
}
