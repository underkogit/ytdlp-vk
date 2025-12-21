use std::fs::File;
use std::path::Path;
use std::{fs, io};
use zip::ZipArchive;

fn ensure_parent(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Извлекает из zip-архива только файлы, путь которых начинается с `target_prefix`.
/// `target_prefix` ожидается с forward-slashes и без ведущего слэша,
/// например: "ffmpeg-master-latest-win64-lgpl-shared/bin/"
pub fn extract_prefix_from_zip(
    zip_path: &Path,
    dest: &Path,
    target_prefix: &str,
) -> zip::result::ZipResult<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name();

        // Нормализуем: zip использует '/' в имени
        if name.starts_with(target_prefix) {
            // Получаем относительный путь внутри целевой папки (удаляем префикс)
            let rel_path = &name[target_prefix.len()..];

            // Пропускаем пустые (например, если сам префикс указывает на директорию)
            if rel_path.is_empty() {
                continue;
            }

            let out_path = dest.join(rel_path);

            if entry.is_dir() {
                fs::create_dir_all(&out_path)?;
            } else {
                // Создаем родительские директории, если нужно
                ensure_parent(&out_path).map_err(|e| zip::result::ZipError::Io(e))?;

                let mut outfile =
                    File::create(&out_path).map_err(|e| zip::result::ZipError::Io(e))?;
                io::copy(&mut entry, &mut outfile).map_err(|e| zip::result::ZipError::Io(e))?;

                // Сохраняем unix-пермиссии, если есть (опционально)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = entry.unix_mode() {
                        fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))
                            .map_err(|e| zip::result::ZipError::Io(e))?;
                    }
                }
            }
        }
    }

    Ok(())
}
