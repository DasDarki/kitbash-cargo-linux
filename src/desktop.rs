use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use image::imageops::FilterType;

use crate::paths::{APP_ID, DESKTOP_FILE_NAME, PROTOCOL, Paths};

const ICON_SIZES: &[u32] = &[16, 24, 32, 48, 64, 128, 256, 512];

pub fn install_icons(paths: &Paths, source_icon: &Path) -> Result<()> {
    let icon = image::open(source_icon)
        .with_context(|| format!("cannot load icon {}", source_icon.display()))?;
    for size in ICON_SIZES {
        let directory = paths
            .icon_theme_dir()
            .join(format!("{size}x{size}"))
            .join("apps");
        fs::create_dir_all(&directory)?;
        icon.resize_exact(*size, *size, FilterType::Lanczos3)
            .save(directory.join(format!("{APP_ID}.png")))?;
    }
    run_quietly("gtk-update-icon-cache", &["-f", "-t", &paths.icon_theme_dir().to_string_lossy()]);
    Ok(())
}

pub fn remove_icons(paths: &Paths) {
    for size in ICON_SIZES {
        let icon = paths
            .icon_theme_dir()
            .join(format!("{size}x{size}"))
            .join("apps")
            .join(format!("{APP_ID}.png"));
        fs::remove_file(icon).ok();
    }
}

pub fn install_desktop_entry(paths: &Paths, version: &str) -> Result<()> {
    let loader = paths.loader_binary();
    let entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.5\n\
         Name=Cargo by KitBash\n\
         GenericName=3D Asset Library\n\
         Comment=Browse and import KitBash3D assets (Linux loader, Cargo {version})\n\
         Exec={exec} launch %U\n\
         TryExec={exec}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         StartupNotify=true\n\
         StartupWMClass={APP_ID}\n\
         Categories=Graphics;3DGraphics;\n\
         Keywords=kitbash;cargo;3d;assets;blender;\n\
         MimeType=x-scheme-handler/{PROTOCOL};\n\
         SingleMainWindow=true\n",
        exec = quote_exec_argument(&loader.to_string_lossy()),
    );
    fs::create_dir_all(paths.applications_dir())?;
    fs::write(paths.desktop_file(), entry)?;
    let applications = paths.applications_dir().to_string_lossy().into_owned();
    run_quietly("update-desktop-database", &[&applications]);
    run_quietly(
        "xdg-mime",
        &["default", DESKTOP_FILE_NAME, &format!("x-scheme-handler/{PROTOCOL}")],
    );
    run_quietly("kbuildsycoca6", &[]);
    Ok(())
}

pub fn remove_desktop_entry(paths: &Paths) {
    fs::remove_file(paths.desktop_file()).ok();
    let applications = paths.applications_dir().to_string_lossy().into_owned();
    run_quietly("update-desktop-database", &[&applications]);
    run_quietly("kbuildsycoca6", &[]);
}

fn quote_exec_argument(argument: &str) -> String {
    let needs_quotes = argument
        .chars()
        .any(|character| character.is_whitespace() || "\"'\\><~|&;$*?#()`".contains(character));
    if !needs_quotes {
        return argument.to_string();
    }
    let escaped: String = argument
        .chars()
        .flat_map(|character| match character {
            '"' | '`' | '$' | '\\' => vec!['\\', character],
            _ => vec![character],
        })
        .collect();
    format!("\"{escaped}\"").replace('\\', "\\\\")
}

fn run_quietly(program: &str, arguments: &[&str]) {
    let _ = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
