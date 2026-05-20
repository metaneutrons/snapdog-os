// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Fabian Schmieder

//! System operations — reads/writes config files, calls systemctl, etc.

use anyhow::Result;

use crate::routes::{
    AudioInfo, AutoUpdateConfig, ClientConfig, DacOverlay, DiscoveredServer, EthernetConfig,
    EthernetInfo, LogsResponse, NetworkOverview, ScanServersResponse, SshConfig, SystemInfo,
    TimezoneInfo, UpdateCheckResponse, UpdateStatus, WifiInfo, WifiNetwork, WifiScanResult,
};

// --- System ---

pub async fn get_system_info() -> SystemInfo {
    let hostname = read_file("/etc/hostname").await.unwrap_or_default();
    let version = read_file("/etc/snapdog-os.version")
        .await
        .unwrap_or_default();
    let channel = read_file("/etc/snapdog-os.channel")
        .await
        .unwrap_or_else(|_| "stable".into());
    let uptime = get_uptime().await;

    SystemInfo {
        hostname: hostname.trim().to_string(),
        version: version.trim().to_string(),
        channel: channel.trim().to_string(),
        uptime_seconds: uptime,
        pi_version: read_file("/etc/raspberrypi.version")
            .await
            .unwrap_or_default()
            .trim()
            .parse()
            .unwrap_or(4),
    }
}

pub async fn set_system(hostname: Option<String>, channel: Option<String>) -> Result<()> {
    if let Some(h) = hostname {
        run_cmd("hostnamectl", &["set-hostname", &h]).await?;
    }
    if let Some(c) = channel {
        tokio::fs::write("/etc/snapdog-os.channel", &c).await?;
    }
    Ok(())
}

pub async fn reboot() {
    let _ = run_cmd("systemctl", &["reboot"]).await;
}

pub async fn trigger_update() -> Result<()> {
    run_cmd("/opt/snapdog/bin/update", &["--reboot"]).await?;
    Ok(())
}

// --- Network ---

pub fn get_network_overview() -> NetworkOverview {
    NetworkOverview {
        ethernet: get_ethernet(),
        wifi: get_wifi(),
    }
}

pub fn get_ethernet() -> EthernetInfo {
    // TODO: parse networkctl/ip addr for eth0
    EthernetInfo {
        connected: false,
        mode: "dhcp".into(),
        ip: String::new(),
        subnet: String::new(),
        gateway: String::new(),
        dns: String::new(),
    }
}

pub async fn set_ethernet(config: EthernetConfig) -> Result<()> {
    let static_cfg = if config.mode == "static" {
        Some(crate::network::StaticConfig {
            ip: config.ip.unwrap_or_default(),
            subnet: config.subnet.unwrap_or_else(|| "255.255.255.0".into()),
            gateway: config.gateway.unwrap_or_default(),
            dns: config.dns.unwrap_or_default(),
        })
    } else {
        None
    };
    crate::network::configure_ethernet(static_cfg.as_ref()).await
}

pub fn get_wifi() -> WifiInfo {
    // TODO: parse wpa_cli status + ip addr
    WifiInfo {
        connected: false,
        ssid: String::new(),
        ip: String::new(),
        subnet: String::new(),
        gateway: String::new(),
        dns: String::new(),
        signal: 0,
        mode: "dhcp".into(),
    }
}

pub async fn set_wifi(
    ssid: &str,
    password: &str,
    static_cfg: Option<&crate::network::StaticConfig>,
) -> Result<()> {
    crate::network::connect_wifi(ssid, password, static_cfg).await
}

pub async fn delete_wifi() -> Result<()> {
    crate::network::disconnect_wifi().await
}

pub async fn wifi_scan() -> WifiScanResult {
    let networks = crate::network::scan_networks().await.unwrap_or_default();
    WifiScanResult {
        networks: networks
            .into_iter()
            .map(|n| WifiNetwork {
                ssid: n.ssid,
                signal: n.signal,
                security: n.security,
            })
            .collect(),
    }
}

// --- Audio ---

const AVAILABLE_OVERLAYS: &[(&str, &str)] = &[
    ("hifiberry-dacplus", "HiFiBerry DAC+/DAC2 Pro"),
    ("hifiberry-amp3", "HiFiBerry Amp3"),
    ("allo-boss-dac-pcm512x-audio", "Allo Boss DAC"),
    ("iqaudio-dacplus", "IQAudio DAC+"),
    ("justboom-dac", "JustBoom DAC"),
    ("max98357a", "MAX98357A (Adafruit, Google AIY)"),
    ("googlevoicehat-soundcard", "Google AIY Voice HAT"),
    ("", "Auto-detect (HAT EEPROM)"),
];

pub async fn get_audio() -> AudioInfo {
    let overlay = crate::config_txt::get_audio_overlay()
        .await
        .unwrap_or_default();
    let detected = read_file("/proc/asound/card0/id")
        .await
        .unwrap_or_default()
        .trim()
        .to_string();

    AudioInfo {
        overlay,
        detected_card: detected,
        soundcard: "hw:0".into(),
        available_overlays: AVAILABLE_OVERLAYS
            .iter()
            .map(|(id, name)| DacOverlay {
                id: (*id).into(),
                name: (*name).into(),
            })
            .collect(),
    }
}

pub async fn set_audio_overlay(overlay: &str) -> Result<()> {
    crate::config_txt::set_audio_overlay(overlay).await
}

// --- Client ---

pub async fn get_client() -> ClientConfig {
    let defaults = read_file("/etc/default/snapdog-client")
        .await
        .unwrap_or_default();
    let args = parse_client_args(&defaults);
    let running = run_cmd("systemctl", &["is-active", "snapdog-client"])
        .await
        .is_ok();
    let available_soundcards = list_soundcards().await;

    ClientConfig {
        server_url: args.server_url,
        host_id: args.host_id,
        soundcard: args.soundcard,
        mixer: args.mixer,
        latency: args.latency,
        mdns_name: "_snapdog._tcp".into(),
        running,
        available_soundcards,
    }
}

pub async fn set_client(config: ClientConfig) -> Result<()> {
    let mut args = Vec::new();
    if !config.server_url.is_empty() {
        args.push(config.server_url);
    }
    if !config.host_id.is_empty() {
        args.push(format!("--hostID {}", config.host_id));
    }
    if config.soundcard != "default" && !config.soundcard.is_empty() {
        args.push(format!("--soundcard {}", config.soundcard));
    }
    if config.mixer != "software" && !config.mixer.is_empty() {
        args.push(format!("--mixer {}", config.mixer));
    }
    if config.latency != 0 {
        args.push(format!("--latency {}", config.latency));
    }

    let content = format!("SNAPDOG_CLIENT_ARGS=\"{}\"\n", args.join(" "));
    tokio::fs::write("/etc/default/snapdog-client", content).await?;
    run_cmd("systemctl", &["restart", "snapdog-client"]).await?;

    // Sync hostname with host_id
    if !config.host_id.is_empty() {
        let _ = run_cmd("hostnamectl", &["set-hostname", &config.host_id]).await;
    }

    Ok(())
}

// --- SSH ---

pub async fn get_ssh() -> SshConfig {
    let enabled = run_cmd("systemctl", &["is-active", "sshd"]).await.is_ok();
    let keys = read_file("/root/.ssh/authorized_keys")
        .await
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();

    SshConfig {
        enabled,
        pubkeys: keys,
    }
}

pub async fn set_ssh(config: SshConfig) -> Result<()> {
    if config.enabled {
        // Harden sshd: pubkey only, no password auth
        let sshd_config = "\
PasswordAuthentication no\n\
KbdInteractiveAuthentication no\n\
PermitRootLogin prohibit-password\n\
";
        tokio::fs::create_dir_all("/etc/ssh/sshd_config.d").await?;
        tokio::fs::write("/etc/ssh/sshd_config.d/snapdog.conf", sshd_config).await?;
        run_cmd("systemctl", &["enable", "--now", "sshd"]).await?;
    } else {
        run_cmd("systemctl", &["disable", "--now", "sshd"]).await?;
    }

    tokio::fs::create_dir_all("/root/.ssh").await?;
    let keys = config.pubkeys.join("\n") + "\n";
    tokio::fs::write("/root/.ssh/authorized_keys", keys).await?;
    run_cmd("chmod", &["600", "/root/.ssh/authorized_keys"]).await?;

    Ok(())
}

// --- OTA Update Check ---

const UPDATE_BASE_URL: &str = "https://update.snapdog.cc/os";

pub async fn check_update() -> UpdateCheckResponse {
    let current = read_file("/etc/snapdog-os.version")
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let channel = read_file("/etc/snapdog-os.channel")
        .await
        .unwrap_or_else(|_| "stable".into())
        .trim()
        .to_string();
    let pi_version = read_file("/etc/raspberrypi.version")
        .await
        .unwrap_or_else(|_| "4".into())
        .trim()
        .to_string();

    let url = format!("{UPDATE_BASE_URL}/metadata/{channel}/pi{pi_version}.json");

    let latest = fetch_latest_version(&url).await.unwrap_or_default();
    let available = !latest.is_empty() && semver_gt(&latest, &current);
    let is_downgrade = !latest.is_empty() && semver_gt(&current, &latest);
    let installable = !latest.is_empty() && latest != current;

    UpdateCheckResponse {
        available,
        installable,
        current_version: current,
        latest_version: latest,
        channel,
        is_downgrade,
    }
}

fn semver_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };
    parse(a) > parse(b)
}

pub async fn get_update_status() -> UpdateStatus {
    // Check if update is in progress (updater.tar.gz exists)
    let downloading = tokio::fs::metadata("/data/updater.tar.gz").await.is_ok();
    // Check if we rolled back (previous version file exists and matches current)
    let rolled_back = tokio::fs::read_to_string("/etc/snapdog-os.version.previous")
        .await
        .ok()
        .and_then(|prev| {
            let current = std::fs::read_to_string("/etc/snapdog-os.version").unwrap_or_default();
            // If previous > current, we rolled back
            if prev.trim() > current.trim() {
                Some(true)
            } else {
                None
            }
        })
        .unwrap_or(false);

    let phase = if downloading {
        "installing".to_string()
    } else {
        "idle".to_string()
    };

    UpdateStatus {
        phase,
        progress: None,
        rolled_back,
    }
}

async fn fetch_latest_version(url: &str) -> Result<String> {
    let output = tokio::process::Command::new("curl")
        .args(["-sf", "-m", "10", url])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("failed to fetch update metadata");
    }

    // Parse {"version":"20260519","url":"..."}
    let body = String::from_utf8_lossy(&output.stdout);
    let version = body
        .split("\"version\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(2))
        .unwrap_or("")
        .to_string();

    Ok(version)
}

// --- Factory Reset ---

pub async fn factory_reset() -> Result<()> {
    tracing::warn!("Factory reset initiated");
    // Remove WiFi config
    let _ = tokio::fs::remove_file("/etc/wpa_supplicant/wpa_supplicant-wlan0.conf").await;
    // Remove networkd configs
    let _ = tokio::fs::remove_file("/etc/systemd/network/10-ethernet.network").await;
    let _ = tokio::fs::remove_file("/etc/systemd/network/20-wifi.network").await;
    // Reset client config
    tokio::fs::write("/etc/default/snapdog-client", "SNAPDOG_CLIENT_ARGS=\"\"\n").await?;
    // Reset hostname
    run_cmd("hostnamectl", &["set-hostname", "snapdog"]).await?;
    // Disable SSH
    let _ = run_cmd("systemctl", &["disable", "--now", "sshd"]).await;
    let _ = tokio::fs::remove_file("/root/.ssh/authorized_keys").await;
    // Reset update channel
    tokio::fs::write("/etc/snapdog-os.channel", "stable\n").await?;
    // Reboot
    tracing::info!("Factory reset complete, rebooting");
    run_cmd("systemctl", &["reboot"]).await?;
    Ok(())
}

// --- Logs ---

pub async fn get_logs() -> LogsResponse {
    let output = tokio::process::Command::new("journalctl")
        .args(["--no-pager", "-n", "200", "--output", "short-iso"])
        .output()
        .await
        .ok();

    let lines = output
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    LogsResponse { lines }
}

// --- Timezone ---

pub async fn get_timezone() -> TimezoneInfo {
    let current = tokio::process::Command::new("timedatectl")
        .args(["show", "--property=Timezone", "--value"])
        .output()
        .await
        .ok()
        .map_or_else(
            || "UTC".into(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );

    let available = tokio::process::Command::new("timedatectl")
        .args(["list-timezones"])
        .output()
        .await
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    TimezoneInfo {
        timezone: current,
        available,
    }
}

pub async fn set_timezone(tz: &str) -> Result<()> {
    run_cmd("timedatectl", &["set-timezone", tz]).await
}

// --- Soundcards ---

pub async fn list_soundcards() -> Vec<String> {
    let output = tokio::process::Command::new("aplay")
        .args(["-l"])
        .output()
        .await
        .ok();

    output
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.starts_with("card "))
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// --- Auto-Update Settings ---

const AUTO_UPDATE_CONF: &str = "/etc/snapdog-os.auto-update";
const UPDATER_TIMER: &str = "/etc/systemd/system/updater.timer.d/schedule.conf";

pub async fn get_auto_update() -> AutoUpdateConfig {
    let content = read_file(AUTO_UPDATE_CONF).await.unwrap_or_default();
    let mut config = AutoUpdateConfig {
        enabled: true,
        interval: "daily".into(),
        time: "03:00".into(),
    };
    for line in content.lines() {
        if let Some(v) = line.strip_prefix("enabled=") {
            config.enabled = v.trim() == "true";
        } else if let Some(v) = line.strip_prefix("interval=") {
            config.interval = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("time=") {
            config.time = v.trim().to_string();
        }
    }
    config
}

pub async fn set_auto_update(config: AutoUpdateConfig) -> Result<()> {
    // Persist config
    let content = format!(
        "enabled={}\ninterval={}\ntime={}\n",
        config.enabled, config.interval, config.time
    );
    tokio::fs::write(AUTO_UPDATE_CONF, content).await?;

    // Generate systemd timer schedule
    let calendar = match config.interval.as_str() {
        _ => format!("*-*-* {}:00", config.time),
        "weekly" => format!("Mon {}:00", config.time),
        "monthly" => format!("*-*-01 {}:00", config.time),
    };

    if config.enabled {
        // Write timer override
        if let Some(parent) = std::path::Path::new(UPDATER_TIMER).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let timer_conf = format!("[Timer]\nOnCalendar=\nOnCalendar={calendar}\n");
        tokio::fs::write(UPDATER_TIMER, timer_conf).await?;
        run_cmd("systemctl", &["daemon-reload"]).await?;
        run_cmd("systemctl", &["enable", "--now", "updater.timer"]).await?;
    } else {
        run_cmd("systemctl", &["disable", "--now", "updater.timer"]).await?;
    }

    Ok(())
}

// --- Server Connectivity Test ---

pub async fn test_server(host: &str, port: u16) -> bool {
    let addr = format!("{host}:{port}");
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    .is_ok_and(|r| r.is_ok())
}

// --- mDNS Server Discovery ---

pub async fn scan_servers() -> ScanServersResponse {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("mDNS daemon failed: {e}");
            return ScanServersResponse { servers: vec![] };
        }
    };

    let service_type = "_snapdog._tcp.local.";
    let receiver = mdns.browse(service_type).unwrap_or_else(|e| {
        tracing::error!("mDNS browse failed: {e}");
        mdns.browse("_invalid._tcp.local.").unwrap()
    });

    let mut servers = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);

    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        if timeout.is_zero() {
            break;
        }

        match tokio::time::timeout(
            timeout,
            tokio::task::spawn_blocking({
                let receiver = receiver.clone();
                move || receiver.recv_timeout(std::time::Duration::from_millis(500))
            }),
        )
        .await
        {
            Ok(Ok(Ok(ServiceEvent::ServiceResolved(info)))) => {
                let name = info
                    .get_fullname()
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let host = info
                    .get_addresses()
                    .iter()
                    .next()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default();
                let port = info.get_port();
                if !host.is_empty() {
                    servers.push(DiscoveredServer { name, host, port });
                }
            }
            Ok(Ok(Ok(_))) => {}
            _ => break,
        }
    }

    let _ = mdns.shutdown();
    ScanServersResponse { servers }
}

// --- Helpers ---

async fn read_file(path: &str) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

async fn run_cmd(cmd: &str, args: &[&str]) -> Result<()> {
    let output = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(
            "{} {} failed: {}",
            cmd,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

async fn get_uptime() -> u64 {
    read_file("/proc/uptime")
        .await
        .unwrap_or_default()
        .split_whitespace()
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

struct ParsedClientArgs {
    server_url: String,
    host_id: String,
    soundcard: String,
    mixer: String,
    latency: i32,
}

fn parse_client_args(defaults_file: &str) -> ParsedClientArgs {
    let args_line = defaults_file
        .lines()
        .find(|l| l.starts_with("SNAPDOG_CLIENT_ARGS="))
        .unwrap_or("")
        .trim_start_matches("SNAPDOG_CLIENT_ARGS=")
        .trim_matches('"');

    let parts: Vec<&str> = args_line.split_whitespace().collect();
    let mut result = ParsedClientArgs {
        server_url: String::new(),
        host_id: String::new(),
        soundcard: "default".into(),
        mixer: "software".into(),
        latency: 0,
    };

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--hostID" if i + 1 < parts.len() => {
                result.host_id = parts[i + 1].to_string();
                i += 2;
            }
            "--soundcard" if i + 1 < parts.len() => {
                result.soundcard = parts[i + 1].to_string();
                i += 2;
            }
            "--mixer" if i + 1 < parts.len() => {
                result.mixer = parts[i + 1].to_string();
                i += 2;
            }
            "--latency" if i + 1 < parts.len() => {
                result.latency = parts[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            s if !s.starts_with('-') && result.server_url.is_empty() => {
                result.server_url = s.to_string();
                i += 1;
            }
            _ => i += 1,
        }
    }

    result
}
