use anyhow::{Result, bail};
use serde_json::Value;

use crate::paths::DESKTOP_FILE_NAME;

const LINUX_SUPPORT: &str = include_str!("patches/linux_support.js");

struct Patch {
    name: &'static str,
    find: &'static str,
    replace: &'static str,
    required: bool,
    all_occurrences: bool,
}

const MAIN_PATCHES: &[Patch] = &[
    Patch {
        name: "report linux as a supported platform",
        find: "  throw new Error(`Unsupported Cargo platform: ${process.platform}`);",
        replace: "  if (process.platform === \"linux\") {\n    return \"linux\";\n  }\n  throw new Error(`Unsupported Cargo platform: ${process.platform}`);",
        required: true,
        all_occurrences: false,
    },
    Patch {
        name: "replace the ToDesktop runtime",
        find: "const todesktop = require(\"@todesktop/runtime\");",
        replace: "const todesktop = __kbLinuxToDesktopStub();",
        required: true,
        all_occurrences: false,
    },
    Patch {
        name: "handle deep links of a second instance",
        find: "    if (process.platform === \"win32\") {\n      const deepLinkUrl = argv.find(",
        replace: "    if (process.platform !== \"darwin\") {\n      const deepLinkUrl = argv.find(",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "handle deep links on startup",
        find: "  if (process.platform !== \"win32\") return;\n  const deepLinkArg",
        replace: "  if (process.platform === \"darwin\") return;\n  const deepLinkArg",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "detect running software",
        find: "async function DetectSoftwareRunning(executableName, cargoPlatform2) {\n  let command;\n",
        replace: "async function DetectSoftwareRunning(executableName, cargoPlatform2) {\n  if (cargoPlatform2 === \"linux\") {\n    return __kbLinuxIsSoftwareRunning(executableName);\n  }\n  let command;\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "blender addon folder",
        find: "function getBlenderAddonFolder(platform, version2) {\n",
        replace: "function getBlenderAddonFolder(platform, version2) {\n  if (platform === \"linux\") {\n    return __kbLinuxBlenderAddonFolder(version2, false);\n  }\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "blender install locations",
        find: "    const windowsLocations = await getWindowsBlenderLocations();\n    locations.push(...windowsLocations);\n  }\n",
        replace: "    const windowsLocations = await getWindowsBlenderLocations();\n    locations.push(...windowsLocations);\n  } else if (platform === \"linux\") {\n    locations.push(...__kbLinuxBlenderLocations());\n  }\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "blender install from custom path",
        find: "  if (platform === \"darwin\") {\n    return getMacBlenderInfoFromPath(pathToExec);\n",
        replace: "  if (platform === \"linux\") {\n    return __kbLinuxBlenderInfoFromPath(pathToExec);\n  }\n  if (platform === \"darwin\") {\n    return getMacBlenderInfoFromPath(pathToExec);\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "skip Cinema 4D lookup",
        find: "async function getC4DInstallLocations(platform) {\n",
        replace: "async function getC4DInstallLocations(platform) {\n  if (platform === \"linux\") return [];\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "skip Houdini lookup",
        find: "async function getHoudiniInstallLocations(platform) {\n",
        replace: "async function getHoudiniInstallLocations(platform) {\n  if (platform === \"linux\") return [];\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "skip Maya lookup",
        find: "async function getMayaInstallLocations() {\n",
        replace: "async function getMayaInstallLocations() {\n  if (cargoPlatform === \"linux\") return [];\n",
        required: false,
        all_occurrences: false,
    },
    Patch {
        name: "skip Unreal lookup",
        find: "  async getInstallLocations() {\n    const registryLocations",
        replace: "  async getInstallLocations() {\n    if (this.platform === \"linux\") return [];\n    const registryLocations",
        required: false,
        all_occurrences: false,
    },
];

pub struct PatchReport {
    pub applied: Vec<&'static str>,
    pub skipped: Vec<&'static str>,
}

pub fn patch_main_script(source: &str, extra_blender_versions: &[String]) -> Result<(String, PatchReport)> {
    let mut patched = source.to_string();
    let mut report = PatchReport {
        applied: Vec::new(),
        skipped: Vec::new(),
    };
    if !extra_blender_versions.is_empty() {
        patched = extend_blender_versions(&patched, extra_blender_versions)?;
        report.applied.push("additional Blender versions");
    }
    for patch in MAIN_PATCHES {
        let count = patched.matches(patch.find).count();
        let applicable = if patch.all_occurrences {
            count > 0
        } else {
            count == 1
        };
        if !applicable {
            if patch.required {
                bail!(
                    "required patch \"{}\" does not match this Cargo version ({count} matches)",
                    patch.name
                );
            }
            report.skipped.push(patch.name);
            continue;
        }
        patched = patched.replace(patch.find, patch.replace);
        report.applied.push(patch.name);
    }
    patched.push('\n');
    patched.push_str(LINUX_SUPPORT);
    Ok((patched, report))
}

pub fn is_valid_blender_version(version: &str) -> bool {
    match version.split_once('.') {
        Some((major, minor)) => {
            !major.is_empty()
                && !minor.is_empty()
                && major.chars().all(|character| character.is_ascii_digit())
                && minor.chars().all(|character| character.is_ascii_digit())
        }
        None => false,
    }
}

fn extend_blender_versions(source: &str, versions: &[String]) -> Result<String> {
    const DECLARATION: &str = "const BlenderSupportedVersions = [";
    let Some(start) = source.find(DECLARATION) else {
        bail!("cannot find the list of supported Blender versions");
    };
    let list_start = start + DECLARATION.len();
    let Some(length) = source[list_start..].find(']') else {
        bail!("the list of supported Blender versions is malformed");
    };
    let list_end = list_start + length;
    let existing = &source[list_start..list_end];
    let mut additions = String::new();
    for version in versions {
        if !is_valid_blender_version(version) {
            bail!("\"{version}\" is not a Blender version like 5.3");
        }
        if !existing.contains(&format!("\"{version}\"")) {
            additions.push_str(&format!(", \"{version}\""));
        }
    }
    Ok(format!("{}{additions}{}", &source[..list_end], &source[list_end..]))
}

pub fn patch_package_json(source: &str) -> Result<String> {
    let mut package: Value = serde_json::from_str(source)?;
    let Some(object) = package.as_object_mut() else {
        bail!("package.json is not an object");
    };
    object.insert("desktopName".into(), Value::String(DESKTOP_FILE_NAME.into()));
    Ok(serde_json::to_string_pretty(&package)?)
}
