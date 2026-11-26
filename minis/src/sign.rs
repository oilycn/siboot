//! Step 5: sign the IPA step 1 got (download.rs) for this iPhone, as Side by
//! Side does for another one: register the device, then isideload's
//! `sign_app` with the app group and no Increased Memory Limit
//! (`SideBySideView.signApp`).

use std::path::{Path, PathBuf};

use isideload::sideload::sideloader::MemoryLimit;

use crate::account::Session;
use crate::fail::{Fail, Result};
use crate::tunnel::DeviceInfo;
use crate::ui;

pub struct Signed {
    /// The signed `.app` folder, ready to upload.
    pub app_dir: PathBuf,
    pub bundle_id: String,
    pub name: String,
    /// `CFBundleShortVersionString`, when it has one.
    pub version: Option<String>,
}

/// 从 Apple 9401 错误中提取被占用的 App ID；若其中包含大写字母（例如 `cn.oily.gnoix.VX8N6X9T3K`），
/// 返回其全小写形式（`cn.oily.gnoix.vx8n6x9t3k`），以便直接复用本账号先前注册的小写 App ID 并原地覆盖安装。
fn lowercase_fallback_bundle_id(text: &str) -> Option<String> {
    let marker = "Identifier '";
    let start = text.find(marker)? + marker.len();
    let rest = &text[start..];
    let end = rest.find('\'')?;
    let id = &rest[..end];
    let lower = id.to_ascii_lowercase();
    if !lower.is_empty() && lower != id {
        Some(lower)
    } else {
        None
    }
}

async fn try_sign_with_bundle_id(
    session: &mut Session,
    ipa: &Path,
    device: &DeviceInfo,
    custom_bundle_id: Option<String>,
) -> std::result::Result<(PathBuf, bool), String> {
    let first = session
        .sideloader
        .sign_app(
            ipa.to_path_buf(),
            custom_bundle_id.clone(),
            MemoryLimit::Off,
            true,
            Some((device.name.as_str(), device.udid.as_str())),
            None,
        )
        .await;
    match first {
        Ok(res) => Ok(res),
        Err(e) => {
            let text = crate::error_text::report_text(&e);
            let lower = text.to_ascii_lowercase();
            if lower.contains("app group")
                || lower.contains("application group")
                || lower.contains("error 35")
            {
                ui::warn("应用组 (App Group) 不可用，正在尝试不带应用组继续签名…");
                session
                    .sideloader
                    .sign_app(
                        ipa.to_path_buf(),
                        custom_bundle_id,
                        MemoryLimit::Off,
                        false,
                        Some((device.name.as_str(), device.udid.as_str())),
                        None,
                    )
                    .await
                    .map_err(|e2| crate::error_text::report_text(&e2))
            } else {
                Err(text)
            }
        }
    }
}

pub async fn sign(session: &mut Session, ipa: &Path, device: &DeviceInfo) -> Result<Signed> {
    ui::status("正在签名，请稍候…");
    let (app_dir, _special) = match try_sign_with_bundle_id(session, ipa, device, None).await {
        Ok(res) => res,
        Err(text) => {
            if text.contains("9401") {
                if let Some(lower_id) = lowercase_fallback_bundle_id(&text) {
                    ui::warn(&format!(
                        "检测到云端已存在小写 App ID，自动切换为 {lower_id} 复用已有配置并原地覆盖重签…"
                    ));
                    match try_sign_with_bundle_id(session, ipa, device, Some(lower_id)).await {
                        Ok(res) => res,
                        Err(text2) => return Err(Fail::new(sign_advice(&text2), text2)),
                    }
                } else {
                    return Err(Fail::new(sign_advice(&text), text));
                }
            } else {
                return Err(Fail::new(sign_advice(&text), text));
            }
        }
    };
    let (bundle_id, name, version) = bundle_info(&app_dir)?;
    ui::ok(&format!("signed {name} as {bundle_id}"));
    Ok(Signed {
        app_dir,
        bundle_id,
        name,
        version,
    })
}

/// Advice for the refusals SideInstaller's own signing explains.
fn sign_advice(text: &str) -> String {
    if text.starts_with("device registration failed") {
        "Apple 拒绝将本设备注册到开发者团队，因此无法签发描述文件。".into()
    } else if text.contains("7460") {
        "该账号的云端开发证书数量已达上限 (7460)，请撤销一张不再使用的旧证书后重试。".into()
    } else if text
        .to_ascii_lowercase()
        .contains("maximum number of app ids")
    {
        "该 Apple 账号本周可注册的 App ID 数量已达上限，请等待过期或更换账号。".into()
    } else if text.contains("9401") {
        "App ID 已被占用或存在大小写冲突 (error 9401)，请确认该 Bundle ID 未被其他账号占用。".into()
    } else {
        "签名失败。".into()
    }
}

/// Bundle id, Home Screen name and version of the signed app.
fn bundle_info(app_dir: &Path) -> Result<(String, String, Option<String>)> {
    let info: plist::Dictionary = plist::from_file(app_dir.join("Info.plist"))
        .map_err(|e| Fail::new("The signed app has no readable Info.plist.", e.to_string()))?;
    let text = |key: &str| {
        info.get(key)
            .and_then(|v| v.as_string())
            .map(str::to_string)
    };
    let bundle_id = text("CFBundleIdentifier").ok_or_else(|| {
        Fail::new(
            "The signed app has no bundle identifier.",
            "CFBundleIdentifier missing",
        )
    })?;
    let name = text("CFBundleDisplayName")
        .filter(|n| !n.is_empty())
        .or_else(|| text("CFBundleName"))
        .unwrap_or_else(|| "SideInstaller".into());
    let version = text("CFBundleShortVersionString").filter(|v| !v.is_empty());
    Ok((bundle_id, name, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_id_and_name_come_from_info_plist() {
        let dir = std::env::temp_dir().join(format!(
            "siboot-app-{}/SideInstaller.app",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut info = plist::Dictionary::new();
        info.insert(
            "CFBundleIdentifier".into(),
            "com.frizzlem.sideinstaller.ABCDE12345".into(),
        );
        info.insert("CFBundleName".into(), "SideInstaller".into());
        plist::to_file_xml(dir.join("Info.plist"), &info).unwrap();
        let (id, name, version) = bundle_info(&dir).unwrap();
        assert_eq!(id, "com.frizzlem.sideinstaller.ABCDE12345");
        assert_eq!(name, "SideInstaller");
        assert_eq!(version, None);
        info.insert("CFBundleShortVersionString".into(), "1.7.0".into());
        plist::to_file_xml(dir.join("Info.plist"), &info).unwrap();
        assert_eq!(bundle_info(&dir).unwrap().2.as_deref(), Some("1.7.0"));
    }
}
