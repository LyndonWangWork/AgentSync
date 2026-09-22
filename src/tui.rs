use crate::discovery::scan_configurations;
use crate::importer::{
    import_archive, list_snapshots, restore_snapshot, ConflictStrategy, ImportResult,
};
use crate::packager::{
    build_candidate_manifest, export_to_file, export_to_memory, filter_discovery_files,
};
use crate::transfer::{receive_and_import, start_send_server};
use crate::tree_select::select_items_tree_with_action;
use chrono::Utc;
use colored::Colorize;
use inquire::{Confirm, Select, Text};
use std::fs::File;
use std::io;
use std::path::PathBuf;
use zip::ZipArchive;

pub fn run_interactive_tui() -> io::Result<()> {
    println!("{}", "\n=======================================================".cyan());
    println!(
        "{}",
        crate::i18n::t!(
            "🚀 欢迎使用 AI CLI 全局配置迁移与同步工具 (AgentSync)",
            "🚀 Welcome to AI CLI Configuration Migration & Sync Tool (AgentSync)"
        )
        .bold()
        .green()
    );
    println!("{}", "=======================================================".cyan());

    loop {
        let opt_export = crate::i18n::t!("📦 导出配置为压缩包 (Export)", "📦 Export Configurations to Archive (Export)");
        let opt_import = crate::i18n::t!("📥 从压缩包导入配置 (Import)", "📥 Import Configurations from Archive (Import)");
        let opt_send = crate::i18n::t!("📡 局域网发送配置 (Send)", "📡 LAN Direct Send (Send)");
        let opt_receive = crate::i18n::t!("📥 局域网接收配置 (Receive)", "📥 LAN Direct Receive (Receive)");
        let opt_list = crate::i18n::t!("🔍 查看本机配置资产 (List)", "🔍 View Local Config Assets (List)");
        let opt_backups = crate::i18n::t!("⏪ 管理历史备份快照 (Backups)", "⏪ Manage Backup Snapshots (Backups)");
        let opt_lang = crate::i18n::t!("🌐 切换界面语言 (Language)", "🌐 Switch Interface Language (语言)");
        let opt_exit = crate::i18n::t!("🚪 退出 (Exit)", "🚪 Exit (Exit)");

        let options = vec![
            opt_export,
            opt_import,
            opt_send,
            opt_receive,
            opt_list,
            opt_backups,
            opt_lang,
            opt_exit,
        ];

        let menu_prompt = crate::i18n::t!("请选择操作:", "Please select an action:");
        let choice: Result<&str, _> = Select::new(menu_prompt, options).prompt();
        match choice {
            Ok(opt) if opt.starts_with("📦") => handle_export()?,
            Ok(opt) if opt.starts_with("📥 从") || opt.starts_with("📥 Import") => handle_import()?,
            Ok(opt) if opt.starts_with("📡") => handle_send()?,
            Ok(opt) if opt.starts_with("📥 局") || opt.starts_with("📥 LAN") => handle_receive()?,
            Ok(opt) if opt.starts_with("🔍") => handle_list()?,
            Ok(opt) if opt.starts_with("⏪") => handle_backups()?,
            Ok(opt) if opt.starts_with("🌐") => handle_switch_language()?,
            _ => {
                println!("{}", crate::i18n::t!("已退出。", "Exited."));
                break;
            }
        }
    }
    Ok(())
}

fn handle_switch_language() -> io::Result<()> {
    let opt_zh: &str = "🇨🇳 中文 (Chinese)";
    let opt_en: &str = "🇺🇸 English (英文)";
    let opt_auto: &str = crate::i18n::t!("🔄 跟随系统语言 (System Default)", "🔄 Follow System Default (跟随系统)");
    let opt_back: &str = crate::i18n::t!("🔙 返回上一级", "🔙 Back");
    let lang_options = vec![opt_zh, opt_en, opt_auto, opt_back];
    let prompt_msg = crate::i18n::t!("请选择界面语言:", "Select display language:");
    let choice: Result<&str, _> = Select::new(prompt_msg, lang_options).prompt();
    if let Ok(c) = choice {
        if c.starts_with("🇨🇳") {
            crate::i18n::set_language("zh");
            println!("{} 已切换为中文", "✅".green());
        } else if c.starts_with("🇺🇸") {
            crate::i18n::set_language("en");
            println!("{} Switched to English", "✅".green());
        } else if c.starts_with("🔄") {
            crate::i18n::set_language("auto");
            println!(
                "{} {}",
                "✅".green(),
                crate::i18n::t!("已切换为跟随系统语言", "Switched to follow system language")
            );
        }
    }
    Ok(())
}

fn handle_export() -> io::Result<()> {
    println!(
        "\n{}",
        crate::i18n::t!(
            "正在扫描本机已安装的 AI 工具与配置资产...",
            "Scanning locally installed AI tools and configuration assets..."
        )
    );
    let discovery = scan_configurations(None)?;
    if discovery.files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("本机未发现任何配置资产。", "No configuration assets discovered on this machine.")
        );
        return Ok(());
    }

    let candidate_manifest = build_candidate_manifest(&discovery);
    let filter = match select_items_tree_with_action(
        &candidate_manifest,
        crate::i18n::t!("导出", "Export"),
    )? {
        Some(f) => f,
        None => return Ok(()),
    };

    let selected_files = filter_discovery_files(&discovery, &filter);
    if selected_files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("未选择任何配置项，取消导出。", "No items selected, export cancelled.")
        );
        return Ok(());
    }

    let default_name = format!("agentsync-export-{}.zip", Utc::now().format("%Y%m%d-%H%M%S"));
    let output_path_str = Text::new(crate::i18n::t!("导出压缩包路径:", "Export archive path:"))
        .with_default(&default_name)
        .with_help_message(crate::i18n::t!(
            "按 Enter 直接使用默认文件名，或输入新的保存路径和文件名",
            "Press Enter to use default filename, or enter custom path and filename"
        ))
        .prompt()
        .map_err(io::Error::other)?;

    let trimmed = output_path_str.trim();
    let final_name = if trimmed.is_empty() {
        default_name
    } else {
        trimmed.to_string()
    };
    let output_path = PathBuf::from(final_name);
    export_to_file(&output_path, &discovery, &selected_files)?;

    println!(
        "\n{} {}",
        "🎉 ".green().bold(),
        crate::i18n::t!(
            format!(
                "成功导出 {} 个文件至: {}",
                selected_files.len(),
                output_path.display()
            ),
            format!(
                "Successfully exported {} files to: {}",
                selected_files.len(),
                output_path.display()
            )
        )
    );
    Ok(())
}

fn handle_send() -> io::Result<()> {
    println!(
        "\n{}",
        crate::i18n::t!(
            "正在扫描本机已安装的 AI 工具与配置资产...",
            "Scanning locally installed AI tools and configuration assets..."
        )
    );
    let discovery = scan_configurations(None)?;
    if discovery.files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("本机未发现任何配置资产。", "No configuration assets discovered on this machine.")
        );
        return Ok(());
    }

    let candidate_manifest = build_candidate_manifest(&discovery);
    let filter = match select_items_tree_with_action(
        &candidate_manifest,
        crate::i18n::t!("发送", "Send"),
    )? {
        Some(f) => f,
        None => return Ok(()),
    };

    let selected_files = filter_discovery_files(&discovery, &filter);
    if selected_files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("未选择任何配置项，取消直传。", "No items selected, direct transfer cancelled.")
        );
        return Ok(());
    }

    let port_str = Text::new(crate::i18n::t!("服务绑定端口:", "Service bind port:"))
        .with_default("38992")
        .prompt()
        .map_err(io::Error::other)?;
    let port = port_str.trim().parse::<u16>().unwrap_or(38992);

    let nopin = Confirm::new(crate::i18n::t!(
        "是否开启免密直传 (无须 PIN 码直接拉取)？",
        "Enable PIN-free direct transfer (allow download without PIN)?"
    ))
    .with_default(false)
    .prompt()
    .unwrap_or(false);

    let once = Confirm::new(crate::i18n::t!(
        "是否单次传输模式 (首个客户端下载完成后自动关闭服务)？",
        "Single-transfer mode (shut down service after first client download)?"
    ))
    .with_default(false)
    .prompt()
    .unwrap_or(false);

    let archive_bytes = export_to_memory(&discovery, &selected_files)?;
    start_send_server(archive_bytes, Some(port), None, nopin, once)
}

fn handle_import() -> io::Result<()> {
    let file_str = Text::new(crate::i18n::t!(
        "待导入的 zip 归档包路径:",
        "Path of zip archive to import:"
    ))
    .prompt()
    .map_err(io::Error::other)?;
    let path = PathBuf::from(file_str.trim());

    if !path.exists() {
        eprintln!(
            "{} {}: {}",
            "❌ ".red(),
            crate::i18n::t!("文件不存在", "File does not exist"),
            path.display()
        );
        return Ok(());
    }

    let file = File::open(&path)?;
    let archive = ZipArchive::new(file)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let opt_prompt = crate::i18n::t!(
        "💬 逐个文件交互询问 (Prompt)",
        "💬 Prompt for each file (Prompt)"
    );
    let opt_force = crate::i18n::t!(
        "⚡ 强制以归档包为准覆盖 (Force)",
        "⚡ Overwrite with archive file (Force)"
    );
    let opt_skip = crate::i18n::t!(
        "⏭️ 跳过已有文件保持本机原样 (Skip)",
        "⏭️ Skip existing files (Skip)"
    );

    let strategy_choice = Select::new(
        crate::i18n::t!(
            "遇到同名文件冲突时的处理策略:",
            "Conflict resolution strategy:"
        ),
        vec![opt_prompt, opt_force, opt_skip],
    )
    .prompt()
    .map_err(io::Error::other)?;

    let strategy = match strategy_choice {
        s if s.starts_with("⚡") => ConflictStrategy::Force,
        s if s.starts_with("⏭️") => ConflictStrategy::Skip,
        _ => ConflictStrategy::Prompt,
    };

    let do_backup = Confirm::new(crate::i18n::t!(
        "是否在导入前自动创建备份快照？",
        "Create backup snapshot before importing?"
    ))
    .with_default(true)
    .prompt()
    .unwrap_or(true);

    let result = import_archive(archive, strategy, do_backup, true, None)?;
    print_import_report(&result);
    Ok(())
}

fn handle_receive() -> io::Result<()> {
    let addr = Text::new(crate::i18n::t!(
        "请输入发送端网络地址 (例如 192.168.1.50 或 192.168.1.50:38992):",
        "Enter sender address (e.g. 192.168.1.50 or 192.168.1.50:38992):"
    ))
    .prompt()
    .map_err(io::Error::other)?;

    let nopin = Confirm::new(crate::i18n::t!(
        "发送端是否启用了免密模式 (--nopin)？",
        "Has sender enabled PIN-free mode (--nopin)?"
    ))
    .with_default(false)
    .prompt()
    .unwrap_or(false);

    let pin_val = if nopin {
        None
    } else {
        let pin = Text::new(crate::i18n::t!(
            "请输入 6 位连接 PIN 码 (支持字母与数字):",
            "Enter 6-character PIN code:"
        ))
        .prompt()
        .map_err(io::Error::other)?;
        Some(pin.trim().to_string())
    };

    let opt_prompt = crate::i18n::t!(
        "💬 逐个文件交互询问 (Prompt)",
        "💬 Prompt for each file (Prompt)"
    );
    let opt_force = crate::i18n::t!(
        "⚡ 强制以接收内容为准覆盖 (Force)",
        "⚡ Overwrite with received content (Force)"
    );
    let opt_skip = crate::i18n::t!(
        "⏭️ 跳过已有文件保持本机原样 (Skip)",
        "⏭️ Skip existing files (Skip)"
    );

    let strategy_choice = Select::new(
        crate::i18n::t!(
            "遇到同名文件冲突时的处理策略:",
            "Conflict resolution strategy:"
        ),
        vec![opt_prompt, opt_force, opt_skip],
    )
    .prompt()
    .map_err(io::Error::other)?;

    let strategy = match strategy_choice {
        s if s.starts_with("⚡") => ConflictStrategy::Force,
        s if s.starts_with("⏭️") => ConflictStrategy::Skip,
        _ => ConflictStrategy::Prompt,
    };

    let do_backup = Confirm::new(crate::i18n::t!(
        "是否在导入前自动创建备份快照？",
        "Create backup snapshot before importing?"
    ))
    .with_default(true)
    .prompt()
    .unwrap_or(true);

    let result = receive_and_import(&addr, pin_val.as_deref(), strategy, do_backup, true, None)?;
    print_import_report(&result);
    Ok(())
}

fn handle_list() -> io::Result<()> {
    println!(
        "\n{} {}",
        "🔍 ".cyan().bold(),
        crate::i18n::t!(
            "扫描本机已安装的 AI 工具与配置资产:",
            "Scanning locally installed AI tools and configuration assets:"
        )
    );
    let discovery = scan_configurations(None)?;

    for tool in &discovery.tools {
        println!(
            "\n{} ({}: ~/{})",
            tool.name.cyan().bold(),
            crate::i18n::t!("根目录", "Root"),
            tool.base_dir
        );
        for module in &tool.modules {
            println!("  {}", module.name.bold());
            for item in &module.items {
                let desc_str = module
                    .descriptions
                    .get(item)
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                println!("    - {}{}", item, desc_str.dimmed());
            }
        }
    }
    println!(
        "\n{}\n",
        crate::i18n::t!(
            format!(
                "总计发现 {} 款 AI 工具，{} 个相关文件。",
                discovery.tools.len(),
                discovery.files.len()
            ),
            format!(
                "Discovered {} AI tools with {} associated files in total.",
                discovery.tools.len(),
                discovery.files.len()
            )
        )
    );
    Ok(())
}

fn handle_backups() -> io::Result<()> {
    let snapshots = list_snapshots()?;
    if snapshots.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("暂无历史备份快照。", "No backup snapshots found.")
        );
        return Ok(());
    }

    let mut options: Vec<String> = snapshots
        .iter()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
        .collect();
    options.push(crate::i18n::t!("🔙 返回上一级", "🔙 Back").to_string());

    let choice = Select::new(
        crate::i18n::t!(
            "请选择需要恢复的历史快照:",
            "Select backup snapshot to restore:"
        ),
        options,
    )
    .prompt();

    if let Ok(snapshot_name) = choice {
        if snapshot_name.starts_with("🔙") {
            return Ok(());
        }
        for snapshot_path in &snapshots {
            if snapshot_path.file_name().unwrap_or_default().to_string_lossy() == snapshot_name {
                let confirm_prompt = crate::i18n::t!(
                    format!("确定要将配置回滚至快照 {} 吗？", snapshot_name),
                    format!("Are you sure you want to rollback configuration to snapshot {}?", snapshot_name)
                );
                let confirm = Confirm::new(&confirm_prompt)
                    .with_default(false)
                    .prompt()
                    .unwrap_or(false);

                if confirm {
                    let count = restore_snapshot(snapshot_path)?;
                    println!(
                        "{} {}",
                        "✅ ".green().bold(),
                        crate::i18n::t!(
                            format!("已成功恢复 {} 个文件！", count),
                            format!("Successfully restored {} files!", count)
                        )
                    );
                }
                break;
            }
        }
    }
    Ok(())
}

pub fn print_import_report(result: &ImportResult) {
    println!(
        "\n{}",
        crate::i18n::t!(
            "================ 导入执行结果 ================",
            "================ Import Results ================"
        )
        .cyan()
        .bold()
    );
    println!(
        "{} {}",
        crate::i18n::t!("📦 涉及文件总数:", "📦 Total files:"),
        result.total_files
    );
    println!(
        "{} {}",
        crate::i18n::t!("✅ 成功写入/恢复:", "✅ Successfully written/restored:"),
        result.written_files.len().to_string().green()
    );
    if !result.merged_files.is_empty() {
        println!(
            "{} {}",
            crate::i18n::t!("🔌 智能合并文件:", "🔌 Merged files:"),
            result.merged_files.len().to_string().yellow()
        );
    }
    if !result.skipped_files.is_empty() {
        println!(
            "{} {}",
            crate::i18n::t!("⏭️ 自动跳过文件:", "⏭️ Skipped files:"),
            result.skipped_files.len().to_string().dimmed()
        );
    }
    if let Some(ref b_dir) = result.backup_dir {
        println!(
            "{} {}",
            crate::i18n::t!("🛡️ 自动快照路径:", "🛡️ Backup snapshot path:"),
            b_dir.display().to_string().cyan()
        );
    }
    println!("==============================================\n");
}
