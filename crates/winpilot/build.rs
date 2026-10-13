use std::env;
use std::error::Error;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ICON: &str = "../../assets/winpilot.ico";
const PRODUCT: &str = "WinPilot";
const WORKSPACE_INPUTS: [&str; 3] = ["../../crates", "../../Cargo.lock", "../../Cargo.toml"];
const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

struct Version {
    text: String,
    numbers: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    tell_cargo(&format!("cargo:rerun-if-changed={ICON}"));
    tell_cargo("cargo:rerun-if-changed=build.rs");
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    tell_cargo(&format!("cargo:rustc-env=WINPILOT_BUILD_ID={}", build_id(&manifest_dir)?));
    let icon = manifest_dir.join(ICON);
    let out = PathBuf::from(env::var("OUT_DIR")?);
    let manifest = out.join("winpilot.manifest");
    fs::write(&manifest, MANIFEST)?;
    let script = out.join("winpilot.rc");
    fs::write(&script, resource_script(&icon, &manifest, &version()?))?;
    embed_resource::compile(&script, embed_resource::NONE).manifest_optional()?;
    Ok(())
}

#[allow(clippy::print_stdout)]
fn tell_cargo(directive: &str) {
    println!("{directive}");
}

fn build_id(manifest_dir: &Path) -> Result<String, Box<dyn Error>> {
    let mut hasher = DefaultHasher::new();
    for input in WORKSPACE_INPUTS {
        tell_cargo(&format!("cargo:rerun-if-changed={input}"));
        hash_tree(&manifest_dir.join(input), &mut hasher)?;
    }
    let built = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    Ok(format!("{}+{built}.{:016x}", env::var("CARGO_PKG_VERSION")?, hasher.finish()))
}

fn hash_tree(path: &Path, hasher: &mut DefaultHasher) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        path.file_name().hash(hasher);
        fs::read(path)?.hash(hasher);
        return Ok(());
    }
    let mut entries: Vec<PathBuf> =
        fs::read_dir(path)?.map(|entry| entry.map(|found| found.path())).collect::<Result<_, _>>()?;
    entries.sort();
    for entry in entries {
        hash_tree(&entry, hasher)?;
    }
    Ok(())
}

fn version() -> Result<Version, Box<dyn Error>> {
    let part = |name: &str| env::var(name);
    let numbers = format!(
        "{},{},{},0",
        part("CARGO_PKG_VERSION_MAJOR")?,
        part("CARGO_PKG_VERSION_MINOR")?,
        part("CARGO_PKG_VERSION_PATCH")?
    );
    Ok(Version { text: part("CARGO_PKG_VERSION")?, numbers })
}

fn resource_script(icon: &Path, manifest: &Path, version: &Version) -> String {
    let Version { text, numbers } = version;
    [
        format!("1 ICON \"{}\"", rc_path(icon)),
        format!("1 24 \"{}\"", rc_path(manifest)),
        "1 VERSIONINFO".to_string(),
        format!("FILEVERSION {numbers}"),
        format!("PRODUCTVERSION {numbers}"),
        "FILEFLAGSMASK 0x3F".to_string(),
        "FILEFLAGS 0x0".to_string(),
        "FILEOS 0x40004".to_string(),
        "FILETYPE 0x1".to_string(),
        "FILESUBTYPE 0x0".to_string(),
        "BEGIN".to_string(),
        "BLOCK \"StringFileInfo\"".to_string(),
        "BEGIN".to_string(),
        "BLOCK \"040904B0\"".to_string(),
        "BEGIN".to_string(),
        string_value("CompanyName", PRODUCT),
        string_value("FileDescription", PRODUCT),
        string_value("FileVersion", text),
        string_value("InternalName", "winpilot"),
        string_value("LegalCopyright", "MIT License"),
        string_value("OriginalFilename", "winpilot.exe"),
        string_value("ProductName", PRODUCT),
        string_value("ProductVersion", text),
        "END".to_string(),
        "END".to_string(),
        "BLOCK \"VarFileInfo\"".to_string(),
        "BEGIN".to_string(),
        "VALUE \"Translation\", 0x0409, 1200".to_string(),
        "END".to_string(),
        "END".to_string(),
    ]
    .join("\n")
}

fn rc_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn string_value(name: &str, value: &str) -> String {
    format!("VALUE \"{name}\", \"{value}\"")
}
