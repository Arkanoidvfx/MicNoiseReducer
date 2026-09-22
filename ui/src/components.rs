use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const CORE_MANIFEST_URL: &str =
    "https://github.com/Arkanoidvfx/MicNoize/releases/download/runtime-core-v2/components.json";
const RVC_MANIFEST_URL: &str =
    "https://github.com/Arkanoidvfx/MicNoize/releases/download/runtime-rvc-v2.1.4/components.json";
const PUBLIC_KEY: &str = "plpoEiomh7k+cZtpxNJX9Zq2RNv0ugQruiH4lZGazHg=";
/// Device node, instance and hardware id of the signed TAG driver that ships inside the core
/// component; identical to `install-tag.ps1`, which stays the developer path with SDK checks.
const DRIVER_KEY: &str = r"HKLM\SYSTEM\CurrentControlSet\Enum\Root\ThinAudioGateway_4d699d4a\0000";
const DRIVER_INSTANCE: &str = r"Root\ThinAudioGateway_4d699d4a\0000";
const DRIVER_HARDWARE_ID: &str = "ThinAudioGateway_4d699d4a-65a5-40ec-9875-8e6d5fc01e0c";
const DRIVER_DIR: &str = "vendor/tag-2.0.0.1903-demo";
const NO_WINDOW: u32 = 0x08000000; // A GUI parent would otherwise flash a console.
static DONE: AtomicU64 = AtomicU64::new(0);
static TOTAL: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
struct Envelope {
    payload: String,
    signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    archive_sha256: String,
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    url: String,
    size: u64,
    sha256: String,
}

pub fn rvc_installed(root: &Path) -> bool {
    root.join("vendor/vcclient-2.1.4-alpha/dist/main/mnr_vcclient_server.exe")
        .is_file()
}

pub fn core_installed(root: &Path) -> bool {
    root.join("vendor/nvidia-afx-3.0.0/bin/NVAudioEffects.dll")
        .is_file()
        && root
            .join("vendor/tag-2.0.0.1903-demo/apidll/x64/tagapi.dll")
            .is_file()
        && root.join("bin/mic_tag_host.exe").is_file()
}

pub fn driver_installed() -> bool {
    Command::new("reg")
        .args(["query", DRIVER_KEY, "/v", "Service"])
        .creation_flags(NO_WINDOW)
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Installs the signed TAG driver that came with the core component. The user confirms one UAC
/// prompt; no Windows security setting is changed. `install-tag.ps1` stays the developer path.
pub fn install_driver(root: &Path) -> Result<(), String> {
    let manager = root.join(DRIVER_DIR).join("wdmdrvmgr/x64/wdmdrvmgr.exe");
    let inf = root
        .join(DRIVER_DIR)
        .join("driver/ThinAudioGateway_4d699d4a.inf");
    for file in [&manager, &inf] {
        if !file.is_file() {
            return Err(format!("Файл драйвера не найден: {}", file.display()));
        }
    }
    let status = Command::new(powershell())
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(install_script(&manager, &inf))
        .creation_flags(NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Виртуальный микрофон не установлен: нужны права администратора. \
                    Перезапустите Mic Noize и подтвердите запрос Windows."
            .into());
    }
    if !driver_installed() {
        return Err("Установщик драйвера завершился, но устройство не появилось".into());
    }
    Ok(())
}

fn powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// `Start-Process -Verb RunAs` is the only elevation path without a service. The INF argument
/// carries its own quotes: Windows PowerShell does not quote list items that contain spaces,
/// and the component path (`%APPDATA%\Mic Noize\Components`) has one.
fn install_script(manager: &Path, inf: &Path) -> String {
    format!(
        "$ErrorActionPreference='Stop';\
         $p=Start-Process -FilePath '{}' -ArgumentList @('-q','-h','{}','-i','{}','\"{}\"') \
         -Verb RunAs -WindowStyle Hidden -PassThru -Wait;exit $p.ExitCode",
        quoted(manager),
        DRIVER_INSTANCE,
        DRIVER_HARDWARE_ID,
        quoted(inf)
    )
}

fn quoted(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

pub fn progress() -> Option<u8> {
    let total = TOTAL.load(Ordering::Relaxed);
    (total > 0).then(|| ((DONE.load(Ordering::Relaxed).saturating_mul(100) / total).min(100)) as u8)
}

pub fn install_rvc(components: &Path) -> Result<String, String> {
    install(
        RVC_MANIFEST_URL,
        components,
        "vendor/vcclient-2.1.4-alpha/dist/main/mnr_vcclient_server.exe",
        &["vendor/vcclient-2.1.4-alpha"],
    )
}

pub fn install_core(components: &Path) -> Result<String, String> {
    install(
        CORE_MANIFEST_URL,
        components,
        "vendor/nvidia-afx-3.0.0/bin/NVAudioEffects.dll",
        &[
            "vendor/nvidia-afx-3.0.0",
            "vendor/tag-2.0.0.1903-demo",
            "bin/mic_tag_host.exe",
        ],
    )
}

fn install(
    manifest_url: &str,
    components: &Path,
    expected: &str,
    entries: &[&str],
) -> Result<String, String> {
    DONE.store(0, Ordering::Relaxed);
    let mut response = ureq::get(manifest_url).call().map_err(|e| e.to_string())?;
    let envelope: Envelope = serde_json::from_str(
        &response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    verify(&envelope)?;
    let manifest: Manifest = serde_json::from_str(&envelope.payload).map_err(|e| e.to_string())?;
    if manifest.parts.is_empty() {
        return Err("Манифест RVC не содержит частей архива".into());
    }
    TOTAL.store(
        manifest.parts.iter().map(|p| p.size).sum(),
        Ordering::Relaxed,
    );
    let work = components.join(".download-rvc");
    let stage = components.join(".stage-rvc");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let archive = work.join("rvc-runtime.tar.zst");
    let _ = std::fs::remove_file(&archive);
    for (index, part) in manifest.parts.iter().enumerate() {
        let path = work.join(format!("part-{index:03}"));
        download(part, &path)?;
        append(&path, &archive)?;
    }
    check_hash(&archive, &manifest.archive_sha256)?;
    let status = std::process::Command::new("tar.exe")
        .args(["-xf"])
        .arg(&archive)
        .arg("-C")
        .arg(&stage)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Не удалось распаковать RVC runtime".into());
    }
    if !stage.join(expected).is_file() {
        return Err("Архив компонента не содержит ожидаемый файл".into());
    }
    for entry in entries {
        let source = stage.join(entry);
        let destination = components.join(entry);
        if destination.is_dir() {
            std::fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
        } else if destination.exists() {
            std::fs::remove_file(&destination).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(destination.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::rename(source, destination).map_err(|e| e.to_string())?;
    }
    if entries.iter().any(|entry| entry.contains("vcclient")) {
        std::fs::create_dir_all(components.join("vendor/vcclient-2.1.4-alpha/dist/main/model_dir"))
            .map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&stage);
    Ok(manifest.version)
}

fn verify(envelope: &Envelope) -> Result<(), String> {
    let key: [u8; 32] = STANDARD
        .decode(PUBLIC_KEY)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "Неверный публичный ключ")?;
    verify_with_key(envelope, key)
}

fn verify_with_key(envelope: &Envelope, key: [u8; 32]) -> Result<(), String> {
    let signature: [u8; 64] = STANDARD
        .decode(&envelope.signature)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "Неверная подпись манифеста")?;
    VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify(
            envelope.payload.as_bytes(),
            &Signature::from_bytes(&signature),
        )
        .map_err(|_| "Подпись RVC-манифеста не прошла проверку".into())
}

fn download(part: &Part, path: &Path) -> Result<(), String> {
    if cached_part_is_valid(part, path) {
        DONE.fetch_add(part.size, Ordering::Relaxed);
        return Ok(());
    }
    let response = ureq::get(&part.url).call().map_err(|e| e.to_string())?;
    let mut reader = response.into_parts().1.into_reader();
    let mut output = File::create(path).map_err(|e| e.to_string())?;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        DONE.fetch_add(count as u64, Ordering::Relaxed);
    }
    drop(output);
    if path.metadata().map_err(|e| e.to_string())?.len() != part.size {
        return Err("Размер загруженной части RVC не совпадает с манифестом".into());
    }
    check_hash(path, &part.sha256)
}

fn cached_part_is_valid(part: &Part, path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.len() == part.size)
        && check_hash(path, &part.sha256).is_ok()
}

fn append(part: &Path, archive: &Path) -> Result<(), String> {
    let mut input = File::open(part).map_err(|e| e.to_string())?;
    let mut output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(archive)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    Ok(())
}

fn check_hash(path: &Path, expected: &str) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash).map_err(|e| e.to_string())?;
    let actual = hex::encode(hash.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!("SHA-256 не совпадает: {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn component_manifest_signature_rejects_tampering() {
        let signing = SigningKey::from_bytes(&[7; 32]);
        let payload = r#"{"version":"test"}"#;
        let envelope = Envelope {
            payload: payload.into(),
            signature: STANDARD.encode(signing.sign(payload.as_bytes()).to_bytes()),
        };
        assert!(verify_with_key(&envelope, signing.verifying_key().to_bytes()).is_ok());
        let tampered = Envelope {
            payload: "{}".into(),
            ..envelope
        };
        assert!(verify_with_key(&tampered, signing.verifying_key().to_bytes()).is_err());
    }

    #[test]
    fn cached_component_part_requires_matching_hash() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../.tmp/component-cache-test")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"runtime").unwrap();
        let part = Part {
            url: String::new(),
            size: 7,
            sha256: "d92c6a81b2ff50096bcda80885427d1f59a25b5f483f7055523504925d16ab23".into(),
        };
        assert!(cached_part_is_valid(&part, &path));
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(!cached_part_is_valid(&part, &path));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn driver_install_script_quotes_paths_with_spaces() {
        let script = install_script(
            Path::new(r"C:\Program Files\wdmdrvmgr.exe"),
            Path::new(r"C:\Users\a\AppData\Roaming\Mic Noize\Components\tag.inf"),
        );
        assert!(script.contains(r"-FilePath 'C:\Program Files\wdmdrvmgr.exe'"));
        assert!(script.contains(r#"'"C:\Users\a\AppData\Roaming\Mic Noize\Components\tag.inf"'"#));
        assert!(script.contains(DRIVER_HARDWARE_ID));
        assert_eq!(quoted(Path::new(r"C:\it's\x.inf")), r"C:\it''s\x.inf");
    }
}
