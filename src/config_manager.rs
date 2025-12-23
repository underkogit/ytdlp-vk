use anyhow::Context;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{env, fs};

/// Configuration structure for the application
/// Serializes/deserializes to/from JSON format
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// Path where downloaded files will be saved
    pub download_path: String,
}

/// Global static configuration instance
/// Uses OnceCell for thread-safe lazy initialization
static CONFIG: OnceCell<Config> = OnceCell::new();

impl Config {
    /// Creates a new configuration with default values
    /// # Returns
    /// Result containing Config or anyhow::Error
    /// # Examples
    /// ```
    /// let config = Config::new()?;
    /// ```
    pub fn new() -> Result<Self, anyhow::Error> {
        // Try to get username from environment variables
        // Falls back to "User" if both USERNAME and USER are not set
        let username = env::var("USERNAME")
            .or_else(|_| env::var("USER"))
            .unwrap_or_else(|_| "User".to_string());

        // Default download path for Windows systems
        let default_path = format!("C:\\Users\\{}\\Music\\VK Downloads", username);

        Ok(Config {
            download_path: default_path,
        })
    }

    /// Initializes the global configuration variable
    /// Must be called before using Config::get_unwrap()
    /// # Returns
    /// Result<(), anyhow::Error> - Ok if initialization succeeded
    /// # Errors
    /// Returns error if config is already initialized
    pub fn init() -> Result<(), anyhow::Error> {
        let config = Config::load_or_create_internal()?;
        CONFIG
            .set(config)
            .map_err(|_| anyhow::anyhow!("Config is already initialized"))?;
        Ok(())
    }

    /// Internal method for loading existing config or creating a new one
    /// # Returns
    /// Result<Config, anyhow::Error> - Loaded or newly created configuration
    fn load_or_create_internal() -> Result<Self, anyhow::Error> {
        let config_path = "config.json";

        // Check if config file exists
        if !PathBuf::from(config_path).exists() {
            // Create new config with default values
            let config = Config::new()?;

            // Serialize to pretty JSON
            let json = serde_json::to_string_pretty(&config)?;

            // Write to file
            fs::write(config_path, json).context("Failed to write config file")?;

            println!("Created new config file: {}", config_path);
            Ok(config)
        } else {
            // Load existing config file
            let data = fs::read_to_string(config_path).context("Failed to read config file")?;

            // Deserialize from JSON
            let config: Config =
                serde_json::from_str(&data).context("Failed to parse config file")?;

            Ok(config)
        }
    }

    /// Gets the global configuration instance with lazy initialization
    /// Initializes the config if it hasn't been initialized yet
    /// # Returns
    /// Result<&'static Config, anyhow::Error> - Reference to global config
    pub fn get() -> Result<&'static Self, anyhow::Error> {
        CONFIG.get_or_try_init(|| Config::load_or_create_internal())
    }

    /// Gets the global configuration instance without Result wrapper
    /// # Panics
    /// Panics if config is not initialized
    /// # Safety
    /// Should only be called after Config::init() or Config::get()
    pub fn get_unwrap() -> &'static Self {
        CONFIG
            .get()
            .expect("Config is not initialized. Call Config::init() or Config::get() first")
    }

    /// Validates if the download path exists in the filesystem
    /// # Returns
    /// bool - true if path exists, false otherwise
    pub fn validate_path(&self) -> bool {
        PathBuf::from(&self.download_path).exists()
    }
}
