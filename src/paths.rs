use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};

pub const APP_ID: &str = "kitbash-cargo";
pub const DESKTOP_FILE_NAME: &str = "kitbash-cargo.desktop";
pub const APP_BINARY_NAME: &str = "kitbash-cargo";
pub const LOADER_BINARY_NAME: &str = "kitbash-cargo-linux";
pub const PROTOCOL: &str = "cargo";

pub struct Paths {
    pub data_home: PathBuf,
    pub cache_home: PathBuf,
    pub config_home: PathBuf,
}

impl Paths {
    pub fn detect() -> Result<Self> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?;
        let data_home = xdg_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
        let cache_home = xdg_dir("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"));
        let config_home = xdg_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        Ok(Self {
            data_home,
            cache_home,
            config_home,
        })
    }

    pub fn flags_file(&self) -> PathBuf {
        self.config_home.join(format!("{APP_ID}-flags.conf"))
    }

    pub fn install_root(&self) -> PathBuf {
        self.data_home.join(APP_ID)
    }

    pub fn app_dir(&self) -> PathBuf {
        self.install_root().join("app")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.install_root().join("app.staging")
    }

    pub fn previous_dir(&self) -> PathBuf {
        self.install_root().join("app.previous")
    }

    pub fn app_binary(&self) -> PathBuf {
        self.app_dir().join(APP_BINARY_NAME)
    }

    pub fn loader_binary(&self) -> PathBuf {
        self.install_root().join(LOADER_BINARY_NAME)
    }

    pub fn install_info(&self) -> PathBuf {
        self.install_root().join("install.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.cache_home.join(LOADER_BINARY_NAME)
    }

    pub fn desktop_file(&self) -> PathBuf {
        self.data_home.join("applications").join(DESKTOP_FILE_NAME)
    }

    pub fn applications_dir(&self) -> PathBuf {
        self.data_home.join("applications")
    }

    pub fn icon_theme_dir(&self) -> PathBuf {
        self.data_home.join("icons").join("hicolor")
    }
}

fn xdg_dir(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}
