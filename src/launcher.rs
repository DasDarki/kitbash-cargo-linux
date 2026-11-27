use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow};

use crate::installer;
use crate::paths::{DESKTOP_FILE_NAME, Paths};

pub fn launch(paths: &Paths, arguments: Vec<OsString>) -> Result<()> {
    installer::ensure_installed(paths)?;
    let mut command = Command::new(paths.app_binary());
    command
        .current_dir(paths.app_dir())
        .env("CHROME_DESKTOP", DESKTOP_FILE_NAME)
        .env_remove("ELECTRON_RUN_AS_NODE")
        .arg("--ozone-platform-hint=auto")
        .arg("--enable-features=WaylandWindowDecorations")
        .arg("--enable-wayland-ime");
    if is_wayland_session() {
        command.arg("--use-angle=vulkan");
    }
    if needs_no_sandbox(&paths.app_dir().join("chrome-sandbox")) {
        command.arg("--no-sandbox");
    }
    command.args(user_flags(paths));
    command.args(arguments);
    let error = command.exec();
    Err(anyhow!("cannot start {}: {error}", paths.app_binary().display()))
}

fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session == "wayland")
}

fn user_flags(paths: &Paths) -> Vec<String> {
    fs::read_to_string(paths.flags_file())
        .map(|content| {
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn needs_no_sandbox(sandbox_helper: &Path) -> bool {
    if std::env::var_os("KITBASH_CARGO_NO_SANDBOX").is_some() {
        return true;
    }
    if setuid_root(sandbox_helper) {
        return false;
    }
    let disabled = |file: &str, value: &str| {
        fs::read_to_string(file)
            .map(|content| content.trim() == value)
            .unwrap_or(false)
    };
    disabled("/proc/sys/kernel/unprivileged_userns_clone", "0")
        || disabled("/proc/sys/user/max_user_namespaces", "0")
        || disabled("/proc/sys/kernel/apparmor_restrict_unprivileged_userns", "1")
}

fn setuid_root(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.uid() == 0 && metadata.mode() & 0o4000 != 0)
        .unwrap_or(false)
}
