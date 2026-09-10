//! 软件版本检查与更新模块。
//! 支持从 GitHub Releases 查询最新版本、比较语义化版本号、
//! 查找当前平台对应安装包资产，并支持在 CLI 与 GUI 设置窗口中展示。

use serde::{Deserialize, Serialize};

pub const DEFAULT_REPO: &str = "lianchengwu/x-tools";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub html_url: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_name: String,
    pub release_notes: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

use std::sync::RwLock;

static CACHED_UPDATE: RwLock<Option<UpdateCheckResult>> = RwLock::new(None);

pub fn get_cached_update() -> Option<UpdateCheckResult> {
    CACHED_UPDATE.read().ok().and_then(|lock| lock.clone())
}

pub fn set_cached_update(info: UpdateCheckResult) {
    if let Ok(mut lock) = CACHED_UPDATE.write() {
        *lock = Some(info);
    }
}

/// 在后台启动静默版本检查并缓存结果
pub fn spawn_background_update_check() {
    std::thread::Builder::new()
        .name("xtools-bg-update-check".into())
        .spawn(|| {
            if let Ok(result) = check_for_update() {
                if result.has_update {
                    log::info!(
                        "发现新版本: v{} (当前: v{})",
                        result.latest_version,
                        result.current_version
                    );
                    #[cfg(unix)]
                    {
                        let _ = std::process::Command::new("notify-send")
                            .arg("xtools 软件更新")
                            .arg(&format!(
                                "发现新版本 v{}！右键托盘或在设置中查看下载。",
                                result.latest_version
                            ))
                            .spawn();
                    }
                }
                set_cached_update(result);
            }
        })
        .ok();
}

/// 去除版本号前缀 'v' 或 'V'
pub fn normalize_version(tag: &str) -> &str {
    let tag = tag.trim();
    tag.strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag)
}

/// 比较版本号：如果 remote 大于 current 则返回 true
pub fn is_newer_version(current: &str, remote: &str) -> bool {
    let cur_clean = normalize_version(current);
    let rem_clean = normalize_version(remote);

    match (
        semver::Version::parse(cur_clean),
        semver::Version::parse(rem_clean),
    ) {
        (Ok(cur), Ok(rem)) => rem > cur,
        _ => compare_version_segments(cur_clean, rem_clean),
    }
}

fn compare_version_segments(cur: &str, rem: &str) -> bool {
    let cur_parts: Vec<u64> = cur
        .split('.')
        .filter_map(|s| s.split('-').next().unwrap_or("").parse().ok())
        .collect();
    let rem_parts: Vec<u64> = rem
        .split('.')
        .filter_map(|s| s.split('-').next().unwrap_or("").parse().ok())
        .collect();
    if !cur_parts.is_empty() && !rem_parts.is_empty() {
        rem_parts > cur_parts
    } else {
        rem != cur
    }
}

/// 格式化文件字节大小
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// 根据操作系统与架构匹配对应的安装包资产
pub fn find_platform_asset_for<'a>(
    assets: &'a [ReleaseAsset],
    os: &str,
    arch: &str,
) -> Option<&'a ReleaseAsset> {
    let target_sub = match (os, arch) {
        ("linux", "x86_64") => "linux-x86_64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => "",
    };

    if !target_sub.is_empty() {
        if let Some(asset) = assets.iter().find(|a| a.name.contains(target_sub)) {
            return Some(asset);
        }
    }

    if os == "windows" {
        if let Some(asset) = assets.iter().find(|a| a.name.ends_with(".zip")) {
            return Some(asset);
        }
    } else if let Some(asset) = assets.iter().find(|a| a.name.ends_with(".tar.gz")) {
        return Some(asset);
    }

    assets.first()
}

/// 查找适合当前系统运行架构的发布资产
pub fn find_platform_asset<'a>(assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
    find_platform_asset_for(assets, std::env::consts::OS, std::env::consts::ARCH)
}

/// 获取更新检查 API 地址（支持通过环境变量覆写）
pub fn get_update_endpoint() -> String {
    if let Ok(url) = std::env::var("XTOOLS_UPDATE_URL") {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let repo = std::env::var("XTOOLS_UPDATE_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

/// 执行检查新版网络请求
pub fn check_for_update() -> Result<UpdateCheckResult, String> {
    let url = get_update_endpoint();
    check_for_update_at(&url)
}

/// 向指定 API 地址发起版本检查
pub fn check_for_update_at(api_url: &str) -> Result<UpdateCheckResult, String> {
    let current_version = env!("CARGO_PKG_VERSION");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into();

    let user_agent = format!("xtools/{current_version}");
    let mut response = agent
        .get(api_url)
        .header("User-Agent", &user_agent)
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .map_err(|e| format!("网络请求失败: {e}"))?;

    let status = response.status();
    if status == 403 || status == 429 {
        return Err("GitHub API 访问受限（触发速率限制），请稍后再试".to_string());
    }
    if status == 404 {
        return Err("未找到最新发布版本 (404 Not Found)".to_string());
    }
    if !status.is_success() {
        return Err(format!("检查更新失败，服务器返回状态码: {status}"));
    }

    let release: GithubRelease = response
        .body_mut()
        .read_json()
        .map_err(|e| format!("解析版本信息失败: {e}"))?;

    let latest_version = normalize_version(&release.tag_name).to_string();
    let has_update = is_newer_version(current_version, &latest_version);
    let asset = find_platform_asset(&release.assets);

    let result = UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version,
        has_update,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_notes: release.body.unwrap_or_default(),
        release_url: release.html_url,
        published_at: release.published_at,
        download_url: asset.map(|a| a.browser_download_url.clone()),
        asset_name: asset.map(|a| a.name.clone()),
        asset_size: asset.map(|a| a.size),
    };
    set_cached_update(result.clone());
    Ok(result)
}

/// 在系统默认浏览器中打开指定链接
pub fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开浏览器失败: {e}"))?;
        Ok(())
    }
}

/// CLI 命令行检查更新入口
pub fn run_cli_check(json_output: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("正在检查更新...");
    match check_for_update() {
        Ok(result) => {
            if json_output {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            println!("当前版本: v{}", result.current_version);
            println!("最新版本: v{}", result.latest_version);
            if let Some(date) = &result.published_at {
                let date_clean = date.split('T').next().unwrap_or(date);
                println!("发布时间: {date_clean}");
            }

            if result.has_update {
                println!();
                println!("🎉 发现新版本: v{}！", result.latest_version);
                println!("发布名称: {}", result.release_name);
                println!("发布主页: {}", result.release_url);
                if let Some(dl) = &result.download_url {
                    let size_str = result
                        .asset_size
                        .map(|s| format!(" ({})", format_size(s)))
                        .unwrap_or_default();
                    println!("下载地址: {dl}{size_str}");
                }
                if !result.release_notes.trim().is_empty() {
                    println!();
                    println!("更新说明:");
                    println!("--------------------------------------------------");
                    println!("{}", result.release_notes.trim());
                    println!("--------------------------------------------------");
                }
            } else {
                println!();
                println!("✓ 当前已是最新版本");
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("✕ 检查更新失败: {e}");
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v0.7.2"), "0.7.2");
        assert_eq!(normalize_version("V1.0.0"), "1.0.0");
        assert_eq!(normalize_version("0.8.1"), "0.8.1");
        assert_eq!(normalize_version("  v2.3.4  "), "2.3.4");
    }

    #[test]
    fn test_is_newer_version() {
        assert!(!is_newer_version("0.7.2", "0.7.2"));
        assert!(!is_newer_version("0.7.2", "v0.7.2"));
        assert!(is_newer_version("0.7.2", "v0.7.3"));
        assert!(is_newer_version("0.7.2", "0.8.0"));
        assert!(is_newer_version("0.7.2", "1.0.0"));
        assert!(!is_newer_version("0.8.0", "0.7.2"));
        assert!(!is_newer_version("1.0.0", "0.9.9"));
        assert!(is_newer_version("0.7.2-beta.1", "0.7.2"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024 * 15), "15.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 2), "2.0 GB");
    }

    #[test]
    fn test_find_platform_asset() {
        let assets = vec![
            ReleaseAsset {
                name: "xtools-0.8.0-linux-x86_64.tar.gz".into(),
                browser_download_url: "https://example.com/linux.tar.gz".into(),
                size: 15000000,
            },
            ReleaseAsset {
                name: "xtools-0.8.0-windows-x86_64.zip".into(),
                browser_download_url: "https://example.com/win.zip".into(),
                size: 16000000,
            },
        ];

        let linux_asset = find_platform_asset_for(&assets, "linux", "x86_64");
        assert_eq!(
            linux_asset.map(|a| a.name.as_str()),
            Some("xtools-0.8.0-linux-x86_64.tar.gz")
        );

        let win_asset = find_platform_asset_for(&assets, "windows", "x86_64");
        assert_eq!(
            win_asset.map(|a| a.name.as_str()),
            Some("xtools-0.8.0-windows-x86_64.zip")
        );
    }

    #[test]
    fn test_deserialize_github_release() {
        let json_data = r#"{
            "tag_name": "v0.8.0",
            "name": "Release v0.8.0",
            "body": "Release notes here",
            "html_url": "https://github.com/lianchengwu/x-tools/releases/tag/v0.8.0",
            "published_at": "2026-09-10T12:00:00Z",
            "assets": [
                {
                    "name": "xtools-0.8.0-linux-x86_64.tar.gz",
                    "browser_download_url": "https://github.com/releases/download/v0.8.0/xtools-0.8.0-linux-x86_64.tar.gz",
                    "size": 12345678
                }
            ]
        }"#;

        let release: GithubRelease = serde_json::from_str(json_data).unwrap();
        assert_eq!(release.tag_name, "v0.8.0");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].size, 12345678);
    }

    #[test]
    fn test_check_for_update_mock_server_newer_version() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            let body = r#"{
                "tag_name": "v99.0.0",
                "name": "Release v99.0.0",
                "body": "Awesome update",
                "html_url": "https://github.com/lianchengwu/x-tools/releases/tag/v99.0.0",
                "published_at": "2026-09-10T00:00:00Z",
                "assets": [
                    {
                        "name": "xtools-99.0.0-linux-x86_64.tar.gz",
                        "browser_download_url": "https://example.com/dl/xtools-99.0.0.tar.gz",
                        "size": 20480000
                    }
                ]
            }"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let res = check_for_update_at(&format!("http://127.0.0.1:{port}/releases/latest"));
        server_thread.join().unwrap();

        assert!(res.is_ok(), "{:?}", res.err());
        let info = res.unwrap();
        assert!(info.has_update);
        assert_eq!(info.latest_version, "99.0.0");
        assert_eq!(info.release_name, "Release v99.0.0");
        assert_eq!(info.release_notes, "Awesome update");
        assert_eq!(
            info.download_url.as_deref(),
            Some("https://example.com/dl/xtools-99.0.0.tar.gz")
        );
    }

    #[test]
    fn test_check_for_update_mock_server_same_version() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let cur_ver = env!("CARGO_PKG_VERSION");
        let tag = format!("v{cur_ver}");

        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);

            let body = format!(
                r#"{{
                "tag_name": "{tag}",
                "name": "Release {tag}",
                "body": "Current release",
                "html_url": "https://github.com/lianchengwu/x-tools/releases/tag/{tag}",
                "published_at": "2026-09-01T00:00:00Z",
                "assets": []
            }}"#
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });

        let res = check_for_update_at(&format!("http://127.0.0.1:{port}/releases/latest"));
        server_thread.join().unwrap();

        assert!(res.is_ok(), "{:?}", res.err());
        let info = res.unwrap();
        assert!(!info.has_update);
        assert_eq!(info.latest_version, cur_ver);
    }

    #[test]
    fn test_check_for_update_mock_server_errors() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        // Test 403 rate limit
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let server_thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes()).unwrap();
        });

        let res = check_for_update_at(&format!("http://127.0.0.1:{port}/rate-limited"));
        server_thread.join().unwrap();
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("速率限制"));
    }

    #[test]
    fn test_cached_update_roundtrip() {
        let sample = UpdateCheckResult {
            current_version: "0.7.2".into(),
            latest_version: "0.8.0".into(),
            has_update: true,
            release_name: "v0.8.0".into(),
            release_notes: "notes".into(),
            release_url: "https://example.com".into(),
            published_at: Some("2026-09-10".into()),
            download_url: None,
            asset_name: None,
            asset_size: None,
        };
        set_cached_update(sample);
        let cached = get_cached_update();
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert_eq!(cached.latest_version, "0.8.0");
        assert!(cached.has_update);
    }
}
