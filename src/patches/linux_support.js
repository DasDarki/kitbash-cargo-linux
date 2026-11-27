
function __kbLinuxToDesktopStub() {
  const { EventEmitter } = require("events");
  const updater = new EventEmitter();
  updater.checkForUpdates = async () => ({ updateInfo: null });
  updater.setBuildId = () => {};
  updater.restartAndInstall = () => {
    require("electron").dialog.showMessageBox({
      type: "info",
      message: "Updates are handled by the Linux loader",
      detail: "Run \"kitbash-cargo-linux update\" in a terminal to install the latest version."
    });
  };
  return {
    init: () => {},
    autoUpdater: updater,
    runtime: { isInstalledUsingWindowsMSI: () => false }
  };
}

const __kbLinuxBlenderFlatpakId = "org.blender.Blender";

function __kbLinuxHome() {
  return process.env.HOME || require("os").homedir();
}

function __kbLinuxConfigHome() {
  const configured = process.env.XDG_CONFIG_HOME;
  if (configured && require("path").isAbsolute(configured)) {
    return configured;
  }
  return require("path").join(__kbLinuxHome(), ".config");
}

function __kbLinuxBlenderAddonFolder(version, isFlatpak) {
  const path = require("path");
  if (isFlatpak) {
    return path.join(__kbLinuxHome(), ".var", "app", __kbLinuxBlenderFlatpakId, "config", "blender", version);
  }
  return path.join(__kbLinuxConfigHome(), "blender", version);
}

function __kbLinuxIsFlatpakExport(execPath) {
  return require("path").basename(execPath) === __kbLinuxBlenderFlatpakId;
}

function __kbLinuxSupportedVersion(version) {
  return typeof version === "string" && validateBlenderVersion(version);
}

function __kbLinuxVersionFolders(directory) {
  const fs = require("fs");
  const path = require("path");
  try {
    return fs
      .readdirSync(directory)
      .filter((entry) => /^\d+\.\d+$/.test(entry))
      .filter((entry) => fs.existsSync(path.join(directory, entry, "scripts")))
      .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  } catch {
    return [];
  }
}

function __kbLinuxBlenderVersionFromLayout(execPath) {
  const fs = require("fs");
  const path = require("path");
  let resolved = execPath;
  try {
    resolved = fs.realpathSync(execPath);
  } catch {
    return "";
  }
  const directories = [path.dirname(resolved)];
  if (resolved.startsWith("/usr/") || resolved.startsWith("/app/")) {
    directories.push(path.join(path.dirname(path.dirname(resolved)), "share", "blender"));
  }
  for (const directory of directories) {
    const versions = __kbLinuxVersionFolders(directory);
    if (versions.length > 0) {
      return versions[0];
    }
  }
  return "";
}

function __kbLinuxBlenderVersionFromCommand(command, args) {
  try {
    const output = require("child_process").execFileSync(command, args, {
      encoding: "utf8",
      timeout: 30000,
      stdio: ["ignore", "pipe", "ignore"]
    });
    const match = output.match(/(?:Blender|Version:)\s+(\d+\.\d+)/);
    return match ? match[1] : "";
  } catch {
    return "";
  }
}

function __kbLinuxBlenderVersion(execPath) {
  if (__kbLinuxIsFlatpakExport(execPath)) {
    return __kbLinuxBlenderVersionFromCommand("flatpak", ["info", __kbLinuxBlenderFlatpakId]);
  }
  const fromLayout = __kbLinuxBlenderVersionFromLayout(execPath);
  if (fromLayout !== "") {
    return fromLayout;
  }
  return __kbLinuxBlenderVersionFromCommand(execPath, ["--factory-startup", "--version"]);
}

function __kbLinuxBlenderInfoFromPath(execPath) {
  const fs = require("fs");
  const emptyResult = { name: "", version: "", exec: "", appdata: "" };
  try {
    if (!execPath || !fs.existsSync(execPath) || fs.statSync(execPath).isDirectory()) {
      return emptyResult;
    }
    const version = __kbLinuxBlenderVersion(execPath);
    if (!__kbLinuxSupportedVersion(version)) {
      return emptyResult;
    }
    return {
      name: "Blender",
      version,
      exec: execPath,
      appdata: __kbLinuxBlenderAddonFolder(version, __kbLinuxIsFlatpakExport(execPath))
    };
  } catch (error) {
    console.error(`Error getting Linux Blender info from path ${execPath}:`, error);
    return emptyResult;
  }
}

function __kbLinuxSteamLibraries() {
  const fs = require("fs");
  const path = require("path");
  const home = __kbLinuxHome();
  const roots = [
    path.join(home, ".steam", "steam"),
    path.join(home, ".local", "share", "Steam"),
    path.join(home, ".var", "app", "com.valvesoftware.Steam", ".local", "share", "Steam")
  ];
  const libraries = new Set();
  for (const root of roots) {
    if (!fs.existsSync(root)) {
      continue;
    }
    libraries.add(root);
    try {
      const vdf = fs.readFileSync(path.join(root, "steamapps", "libraryfolders.vdf"), "utf8");
      for (const match of vdf.matchAll(/"path"\s+"([^"]+)"/g)) {
        libraries.add(match[1].replace(/\\\\/g, "\\"));
      }
    } catch {
    }
  }
  return [...libraries];
}

function __kbLinuxGlobDirectories(parent, pattern) {
  const fs = require("fs");
  const path = require("path");
  try {
    return fs
      .readdirSync(parent, { withFileTypes: true })
      .filter((entry) => (entry.isDirectory() || entry.isSymbolicLink()) && pattern.test(entry.name))
      .map((entry) => path.join(parent, entry.name));
  } catch {
    return [];
  }
}

function __kbLinuxBlenderCandidates() {
  const path = require("path");
  const home = __kbLinuxHome();
  const candidates = [];
  for (const directory of (process.env.PATH || "").split(":")) {
    if (directory) {
      candidates.push(path.join(directory, "blender"));
    }
  }
  candidates.push("/usr/bin/blender", "/usr/local/bin/blender", "/snap/bin/blender", "/opt/blender/blender");
  const searchRoots = ["/opt", home, path.join(home, "Applications"), path.join(home, "apps"), path.join(home, ".local", "opt"), path.join(home, ".local", "share")];
  for (const root of searchRoots) {
    for (const directory of __kbLinuxGlobDirectories(root, /^blender/i)) {
      candidates.push(path.join(directory, "blender"));
      for (const nested of __kbLinuxGlobDirectories(directory, /^blender/i)) {
        candidates.push(path.join(nested, "blender"));
      }
    }
  }
  for (const library of __kbLinuxSteamLibraries()) {
    candidates.push(path.join(library, "steamapps", "common", "Blender", "blender"));
  }
  candidates.push(
    path.join("/var/lib/flatpak/exports/bin", __kbLinuxBlenderFlatpakId),
    path.join(home, ".local", "share", "flatpak", "exports", "bin", __kbLinuxBlenderFlatpakId)
  );
  return candidates;
}

function __kbLinuxBlenderLocations() {
  const fs = require("fs");
  const seen = new Set();
  const locations = [];
  for (const candidate of __kbLinuxBlenderCandidates()) {
    let key = candidate;
    try {
      if (!fs.existsSync(candidate)) {
        continue;
      }
      key = fs.realpathSync(candidate);
    } catch {
      continue;
    }
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    const info = __kbLinuxBlenderInfoFromPath(candidate);
    if (info.name !== "") {
      locations.push(info);
    }
  }
  return locations;
}

function __kbLinuxIsSoftwareRunning(executableName) {
  const patterns = {
    blender: /^blender$/i,
    Houdini: /^(houdini|houdinifx|houdinicore|happrentice|hindie)(-bin)?$/i,
    maya: /^maya(\.bin)?$/i,
    UnrealEditor: /^UnrealEditor$/
  };
  const escapedName = executableName.replace(/[.*+?^$()|[\]\\{}]/g, "\\$&");
  const pattern = patterns[executableName] || new RegExp("^" + escapedName + "$", "i");
  return new Promise((resolve) => {
    require("child_process").execFile("ps", ["-eo", "comm="], (error, stdout) => {
      if (error) {
        resolve(false);
        return;
      }
      resolve(stdout.split("\n").some((line) => pattern.test(line.trim())));
    });
  });
}
