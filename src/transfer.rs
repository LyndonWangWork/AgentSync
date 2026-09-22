use crate::importer::{import_archive, ConflictStrategy, ImportResult};
use colored::Colorize;
use rand::Rng;
use std::io::{self, Cursor};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tiny_http::{Header, Response, Server, StatusCode};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct LanCandidate {
    pub ip: Ipv4Addr,
    #[allow(dead_code)]
    pub interface_name: String,
    pub is_virtual: bool,
}

pub fn get_lan_candidates() -> Vec<LanCandidate> {
    let mut candidates = Vec::new();
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            if let get_if_addrs::IfAddr::V4(v4) = iface.addr {
                let ip = v4.ip;
                let octets = ip.octets();
                // 排除 127.0.0.0/8 环回与 169.254.0.0/16 链路本地地址
                if octets[0] == 127 || (octets[0] == 169 && octets[1] == 254) {
                    continue;
                }

                let name_lower = iface.name.to_lowercase();
                let is_172_virtual = octets[0] == 172 && (16..=31).contains(&octets[1]);
                let is_virtual = is_172_virtual
                    || name_lower.contains("tun")
                    || name_lower.contains("tap")
                    || name_lower.contains("wsl")
                    || name_lower.contains("vethernet")
                    || name_lower.contains("docker")
                    || name_lower.contains("hyper-v")
                    || name_lower.contains("virtual")
                    || name_lower.contains("vmnet")
                    || name_lower.contains("tailscale")
                    || name_lower.contains("singbox")
                    || name_lower.contains("clash")
                    || name_lower.contains("vpn");

                candidates.push(LanCandidate {
                    ip,
                    interface_name: iface.name,
                    is_virtual,
                });
            }
        }
    }

    // 优先排序：标准物理局域网(192.168.x.x > 10.x.x.x)排在最前，虚拟/容器网段(172.x)与其它网段排在后
    candidates.sort_by_key(|c| {
        let octets = c.ip.octets();
        let ip_score = if octets[0] == 192 && octets[1] == 168 {
            0
        } else if octets[0] == 10 {
            1
        } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
            2
        } else {
            3
        };
        (c.is_virtual, ip_score)
    });

    candidates
}

pub const DEFAULT_PORT: u16 = 38992;
const PIN_CHARSET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

pub fn generate_pin() -> String {
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| {
            let idx = rng.gen_range(0..PIN_CHARSET.len());
            PIN_CHARSET[idx] as char
        })
        .collect()
}

pub fn start_send_server(
    archive_bytes: Vec<u8>,
    port: Option<u16>,
    custom_ip: Option<String>,
    nopin: bool,
    once: bool,
) -> io::Result<()> {
    let port_to_bind = port.unwrap_or(DEFAULT_PORT);
    let bind_addr = format!("0.0.0.0:{}", port_to_bind);
    let server = Server::http(&bind_addr).or_else(|e| {
        // 如果固定默认端口被占用且用户未显式指定，回退至系统随机空闲端口
        if port.is_none() {
            Server::http("0.0.0.0:0")
        } else {
            Err(e)
        }
    }).map_err(|e| io::Error::other(format!("无法启动传输服务: {}", e)))?;

    let actual_port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(port_to_bind);
    let candidates = get_lan_candidates();

    let primary_ip = if let Some(ip) = custom_ip {
        ip
    } else if let Some(first) = candidates.first() {
        first.ip.to_string()
    } else {
        "127.0.0.1".into()
    };

    let pin_str = if nopin {
        String::new()
    } else {
        generate_pin()
    };
    let data = Arc::new(archive_bytes);
    let counter = Arc::new(AtomicUsize::new(0));

    println!("\n{}", "=======================================================".cyan());
    println!(
        "{}",
        crate::i18n::t!(
            "📡 局域网直传服务已启动！",
            "📡 LAN Direct Transfer Service Started!"
        )
        .green()
        .bold()
    );
    println!(
        "{}: {}",
        crate::i18n::t!("服务监听端口   ", "Listening Port  "),
        actual_port.to_string().bold()
    );
    if nopin {
        println!(
            "{}: {}",
            crate::i18n::t!("一次性验证 PIN 码", "One-time PIN Code"),
            crate::i18n::t!("[已免密 (--nopin)]", "[PIN-free (--nopin)]").bold().green()
        );
    } else {
        println!(
            "{}: {}",
            crate::i18n::t!("一次性验证 PIN 码", "One-time PIN Code"),
            pin_str.bold().yellow()
        );
    }

    let cmd_suffix = if nopin {
        "--nopin".to_string()
    } else {
        format!("--pin {}", pin_str)
    };

    println!(
        "\n{}",
        crate::i18n::t!(
            "目标机器连接命令（物理局域网推荐）：",
            "Target machine connection command (physical LAN recommended):"
        )
    );
    if actual_port == DEFAULT_PORT {
        println!(
            "  {}",
            format!("agentsync receive {} {}", primary_ip, cmd_suffix)
                .bold()
                .green()
        );
        println!(
            "  {}",
            crate::i18n::t!(
                format!("(也可完整带端口: agentsync receive {}:{} {})", primary_ip, actual_port, cmd_suffix),
                format!("(or with explicit port: agentsync receive {}:{} {})", primary_ip, actual_port, cmd_suffix)
            )
            .dimmed()
        );
    } else {
        println!(
            "  {}",
            format!("agentsync receive {}:{} {}", primary_ip, actual_port, cmd_suffix)
                .bold()
                .green()
        );
    }

    if candidates.len() > 1 {
        println!(
            "\n{}",
            crate::i18n::t!(
                "检测到本机存在多个网络接口：",
                "Multiple network interfaces detected on this machine:"
            )
        );
        for cand in &candidates {
            let tag = if cand.is_virtual {
                crate::i18n::t!("虚拟网卡/TUN/WSL", "Virtual Adapter/TUN/WSL").yellow()
            } else {
                crate::i18n::t!("物理局域网/Wi-Fi (推荐)", "Physical LAN/Wi-Fi (Recommended)").green().bold()
            };
            println!(
                "  - {}:{}  [{}]",
                cand.ip, actual_port, tag
            );
        }
        println!(
            "{}",
            crate::i18n::t!(
                "若不在同一 Wi-Fi 或局域网，可选用上述对应网段 IP 连接。",
                "If not on the same Wi-Fi/subnet, select the corresponding IP above."
            )
        );
    }

    println!("{}\n", "=======================================================".cyan());
    if once {
        println!(
            "{}",
            crate::i18n::t!(
                "正在等待目标机连接... (单次模式 --once: 首台机器完成后退出)",
                "Waiting for target machine... (Single mode --once: exit after first transfer)"
            )
        );
    } else {
        println!(
            "{}",
            crate::i18n::t!(
                "正在等待目标机连接... (多客户端常驻模式，按 Ctrl+C 退出)",
                "Waiting for target machine... (Resident mode, press Ctrl+C to exit)"
            )
        );
    }

    for request in server.incoming_requests() {
        let client_addr = request
            .remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| crate::i18n::t!("未知", "Unknown").to_string());
        let url = request.url().to_string();
        if !nopin {
            let query_pin = url
                .split('?')
                .nth(1)
                .and_then(|q| {
                    q.split('&')
                        .find(|pair| pair.starts_with("pin="))
                        .map(|pair| pair.trim_start_matches("pin="))
                })
                .unwrap_or_default();

            if !query_pin.eq_ignore_ascii_case(&pin_str) {
                let response = Response::from_string("PIN invalid")
                    .with_status_code(StatusCode(403));
                let _ = request.respond(response);
                println!(
                    "{} [{}] {}",
                    "⚠️ ".yellow(),
                    client_addr,
                    crate::i18n::t!("收到错误的 PIN 验证请求", "Received invalid PIN verification request")
                );
                continue;
            }
        }

        if url.starts_with("/download") {
            let len_str = data.len().to_string();
            let header_type = Header::from_bytes(&b"Content-Type"[..], &b"application/zip"[..]).unwrap();
            let header_len = Header::from_bytes(&b"Content-Length"[..], len_str.as_bytes()).unwrap();
            let header_conn = Header::from_bytes(&b"Connection"[..], &b"close"[..]).unwrap();

            let response = Response::from_data((*data).clone())
                .with_header(header_type)
                .with_header(header_len)
                .with_header(header_conn)
                .with_status_code(StatusCode(200));

            println!(
                "{} [{}] {}",
                "📥 ".cyan(),
                client_addr,
                crate::i18n::t!("正在发送配置数据...", "Sending configuration data...")
            );

            let counter_clone = Arc::clone(&counter);
            let client_addr_clone = client_addr.clone();

            let handle = std::thread::spawn(move || {
                let res = request.respond(response);
                if res.is_ok() {
                    // 确保 TCP 缓冲区完全刷出并完成网络传输，避免过早释放 Socket 导致客户端 10054 ConnectionReset
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let count = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                        "{} [{}] {}",
                        "🎉 ".green().bold(),
                        client_addr_clone,
                        crate::i18n::t!(
                            format!("传输完成！(累计已服务 {} 台机器)", count),
                            format!("Transfer complete! (Served {} machines in total)", count)
                        )
                    );
                }
            });

            if once {
                let _ = handle.join();
                println!(
                    "{} {}",
                    "ℹ️ ".blue(),
                    crate::i18n::t!(
                        "单次传输模式已完成，服务退出。",
                        "Single-transfer mode complete, shutting down service."
                    )
                );
                break;
            }
        } else {
            let response = Response::from_string("OK").with_status_code(StatusCode(200));
            let _ = request.respond(response);
        }
    }

    Ok(())
}

pub fn receive_and_import(
    target_addr: &str,
    pin: Option<&str>,
    strategy: ConflictStrategy,
    do_backup: bool,
    interactive: bool,
    filter: Option<crate::importer::ImportFilter>,
) -> io::Result<ImportResult> {
    let clean_addr = target_addr
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');

    let host_port = if !clean_addr.contains(':') {
        format!("{}:{}", clean_addr, DEFAULT_PORT)
    } else {
        clean_addr.to_string()
    };

    let url = match pin {
        Some(p) if !p.trim().is_empty() => format!("http://{}/download?pin={}", host_port, p.trim()),
        _ => format!("http://{}/download", host_port),
    };

    println!(
        "{} {}",
        "📥 ".cyan(),
        crate::i18n::t!(
            format!("正在连接并拉取配置: {}", url),
            format!("Connecting and downloading configuration: {}", url)
        )
    );

    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::ConnectionRefused,
                crate::i18n::t!(format!("拉取失败: {}", e), format!("Download failed: {}", e)),
            )
        })?;

    if resp.status() != 200 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            crate::i18n::t!(
                format!("服务器响应状态异常: {}", resp.status()),
                format!("Server returned abnormal status: {}", resp.status())
            ),
        ));
    }

    let mut body_bytes = Vec::new();
    let mut reader = resp.into_reader();
    reader.read_to_end(&mut body_bytes)?;

    println!(
        "{} {}",
        "📦 ".green(),
        crate::i18n::t!(
            format!("配置包拉取成功，大小: {} 字节", body_bytes.len()),
            format!("Configuration archive downloaded successfully, size: {} bytes", body_bytes.len())
        )
    );

    let cursor = Cursor::new(body_bytes);
    let archive = ZipArchive::new(cursor).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            crate::i18n::t!(
                format!("无效的 ZIP 格式: {}", e),
                format!("Invalid ZIP format: {}", e)
            ),
        )
    })?;

    import_archive(archive, strategy, do_backup, interactive, filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_pin() {
        let pin = generate_pin();
        assert_eq!(pin.len(), 6);
        for ch in pin.chars() {
            assert!(ch.is_ascii_alphanumeric() && (ch.is_ascii_uppercase() || ch.is_ascii_digit()));
        }
    }

    #[test]
    fn test_pin_case_insensitivity() {
        let pin = "A1B2C3";
        assert!(pin.eq_ignore_ascii_case("a1b2c3"));
        assert!(pin.eq_ignore_ascii_case("A1B2C3"));
    }
}
