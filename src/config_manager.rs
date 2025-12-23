use anyhow::Context;
use serde::{Deserialize, Serialize};
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::{env, fs};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub download_path: String,
}

static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    pub fn new() -> Result<Self, anyhow::Error> {
        let username = env::var("USERNAME")
            .or_else(|_| env::var("USER"))
            .unwrap_or_else(|_| "User".to_string());

        let default_path = format!("C:\\Users\\{}\\Music\\VK Downloads", username);

        Ok(Config {
            download_path: default_path,
        })
    }

    // Исправленный метод, который инициализирует глобальную переменную
    pub fn init() -> Result<(), anyhow::Error> {
        let config = Config::load_or_create_internal()?;
        CONFIG.set(config)
            .map_err(|_| anyhow::anyhow!("Config уже инициализирован"))?;
        Ok(())
    }

    // Внутренний метод для загрузки/создания конфига
    fn load_or_create_internal() -> Result<Self, anyhow::Error> {
        let config_path = "config.json";

        if !PathBuf::from(config_path).exists() {
            let config = Config::new()?;
            let json = serde_json::to_string_pretty(&config)?;
            fs::write(config_path, json).context("Не удалось записать конфиг файл")?;
            println!("Создан новый конфиг файл: {}", config_path);
            Ok(config)
        } else {
            let data = fs::read_to_string(config_path)
                .context("Не удалось прочитать конфиг файл")?;
            let config: Config = serde_json::from_str(&data)
                .context("Не удалось распарсить конфиг файл")?;
            Ok(config)
        }
    }

    // Метод для получения глобального конфига с ленивой инициализацией
    pub fn get() -> Result<&'static Self, anyhow::Error> {
        CONFIG.get_or_try_init(|| {
            Config::load_or_create_internal()
        })
    }

    // Альтернативный вариант без Result
    pub fn get_unwrap() -> &'static Self {
        CONFIG.get().expect("Config не инициализирован. Вызовите Config::init() или Config::get()")
    }

    pub fn validate_path(&self) -> bool {
        PathBuf::from(&self.download_path).exists()
    }
}