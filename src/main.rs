mod descriptions;
mod discovery;
pub mod i18n;
mod importer;
mod model;
mod packager;
mod transfer;
mod tree_select;
mod tui;

use clap::{Args, Parser, Subcommand};
use colored::Colorize;
use discovery::scan_configurations;
use importer::{import_archive, list_snapshots, restore_snapshot, ConflictStrategy};
use model::ToolId;
use packager::{
    build_candidate_manifest, export_to_file, export_to_memory, filter_discovery_files,
};
use std::fs::File;
use std::path::PathBuf;
use transfer::{receive_and_import, start_send_server};
use tree_select::select_items_tree_with_action;
use crossterm::tty::IsTty;
use zip::ZipArchive;

#[derive(Parser, Debug)]
#[command(name = "agentsync")]
#[command(alias = "as")]
#[command(alias = "aitrans")]
#[command(author = "Antigravity Team")]
#[command(version = "0.2.0")]
#[command(about = "AI CLI 全局配置迁移与同步工具 / AI CLI Configuration Migration & Sync Tool", long_about = None)]
struct Cli {
    /// 界面显示语言 / Interface display language (zh, en, auto)
    #[arg(long, global = true, value_parser = ["zh", "en", "auto"], default_value = "auto")]
    lang: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 交互式终端向导 (TUI 模式) / Interactive Terminal Wizard (TUI mode)
    Tui,

    /// 导出配置为压缩包 / Export configurations to archive
    Export(ExportArgs),

    /// 导入并恢复配置归档包 / Import configurations from archive
    Import(ImportArgs),

    /// 启动局域网直传服务 (发送端) / Start LAN direct send server
    Send(SendArgs),

    /// 连接局域网服务拉取并导入配置 (接收端) / Connect to LAN server to receive configurations
    Receive(ReceiveArgs),

    /// 查看本机可迁移的配置资产清单 / List discoverable configuration assets
    List(ListArgs),

    /// 管理历史备份快照 / Manage backup snapshots
    Backup(BackupArgs),
}

#[derive(Args, Debug)]
struct ListArgs {
    /// 指定扫描的 AI 工具 / Specify AI tools to scan (comma-separated, e.g. antigravity,claude,codex,opencode)
    #[arg(short = 't', long)]
    tool: Option<String>,
}

#[derive(Args, Debug)]
struct ExportArgs {
    /// 导出压缩包文件路径 / Output archive file path (default ./agentsync-export-<timestamp>.zip)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 指定包含的 AI 工具 / Specify AI tools to include (comma-separated, e.g. antigravity,claude)
    #[arg(short = 't', long)]
    tool: Option<String>,

    /// 导出全部常规配置模块 (跳过交互选择，不含敏感凭据) / Export all non-sensitive modules (skip interactive tree)
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Args, Debug)]
struct ImportArgs {
    /// 待导入的 .zip 文件路径 / Path to .zip archive file to import
    pub file: PathBuf,

    /// 导入全部包含的配置模块 (跳过交互选择) / Import all modules (skip interactive tree)
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// 遇到冲突时强制覆盖 / Overwrite existing files on conflict
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// 遇到同名文件时跳过 / Skip existing files on conflict
    #[arg(long, default_value_t = false)]
    pub skip_existing: bool,

    /// 导入前跳过创建备份快照 / Skip creating backup snapshot before importing
    #[arg(long, default_value_t = false)]
    pub no_backup: bool,

    /// 自动确认导入，跳过人工交互提示 / Auto-confirm import without prompts
    #[arg(short = 'y', long, default_value_t = false)]
    pub yes: bool,
}

#[derive(Args, Debug)]
struct SendArgs {
    /// 指定服务绑定端口 (默认 38992) / Service bind port (default 38992)
    #[arg(short, long)]
    port: Option<u16>,

    /// 指定对外展示的本机 IP 地址 / Specific host IP to display for connections
    #[arg(long)]
    ip: Option<String>,

    /// 指定包含的 AI 工具 / Specify AI tools to include (comma-separated, e.g. antigravity,claude)
    #[arg(short = 't', long)]
    tool: Option<String>,

    /// 发送全部常规配置模块 (跳过交互选择) / Send all non-sensitive modules (skip interactive tree)
    #[arg(long, default_value_t = false)]
    all: bool,

    /// 禁用连接 PIN 码验证，允许局域网客户端免密直接拉取 / Disable PIN verification (allow PIN-free download)
    #[arg(long, default_value_t = false)]
    nopin: bool,

    /// 首次传输完成后立即退出服务端（单次传输模式） / Single transfer mode (exit after first download)
    #[arg(long, default_value_t = false)]
    once: bool,
}

#[derive(Args, Debug)]
struct ReceiveArgs {
    /// 源机器地址 / Sender machine address (e.g. 192.168.31.222 or 192.168.31.222:38992)
    pub addr: String,

    /// 6 位连接验证 PIN 码 / 6-character connection PIN code
    #[arg(short, long)]
    pub pin: Option<String>,

    /// 无需 PIN 码直接免密连接 / Connect without PIN
    #[arg(long, default_value_t = false)]
    pub nopin: bool,

    /// 接收并导入全部包含的配置模块 (跳过交互选择) / Receive and import all modules (skip interactive tree)
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// 遇到冲突时强制覆盖 / Overwrite existing files on conflict
    #[arg(short, long, default_value_t = false)]
    pub force: bool,

    /// 导入前跳过创建备份快照 / Skip creating backup snapshot before importing
    #[arg(long, default_value_t = false)]
    pub no_backup: bool,

    /// 自动确认导入，跳过人工交互提示 / Auto-confirm import without prompts
    #[arg(short = 'y', long, default_value_t = false)]
    pub yes: bool,
}

#[derive(Args, Debug)]
struct BackupArgs {
    #[command(subcommand)]
    command: BackupCommands,
}

#[derive(Subcommand, Debug)]
enum BackupCommands {
    /// 列出所有可用的历史备份快照 / List all available backup snapshots
    List,

    /// 从指定快照还原配置 / Restore configuration from snapshot
    Restore {
        /// 快照目录名称 / Snapshot directory name (e.g. snapshot-20260921-084444)
        snapshot_name: String,
    },
}

fn parse_tools_arg(tool_str: Option<&str>) -> Option<Vec<ToolId>> {
    tool_str.map(|s| {
        s.split(',')
            .filter_map(|part| part.trim().parse::<ToolId>().ok())
            .collect()
    })
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    i18n::set_language(&cli.lang);

    match cli.command {
        None | Some(Commands::Tui) => tui::run_interactive_tui(),
        Some(Commands::List(args)) => run_list(args),
        Some(Commands::Export(args)) => run_export(args),
        Some(Commands::Import(args)) => run_import(args),
        Some(Commands::Send(args)) => run_send(args),
        Some(Commands::Receive(args)) => run_receive(args),
        Some(Commands::Backup(args)) => run_backup(args),
    }
}

fn run_list(args: ListArgs) -> std::io::Result<()> {
    println!(
        "\n{} {}",
        "🔍 ".cyan().bold(),
        crate::i18n::t!(
            "扫描本机已安装的 AI 工具与配置资产:",
            "Scanning locally installed AI tools and configuration assets:"
        )
    );
    let tools_filter = parse_tools_arg(args.tool.as_deref());
    let discovery = scan_configurations(tools_filter.as_deref())?;

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

fn run_export(args: ExportArgs) -> std::io::Result<()> {
    let tools_filter = parse_tools_arg(args.tool.as_deref());
    let discovery = scan_configurations(tools_filter.as_deref())?;
    if discovery.files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("本机未发现任何配置资产。", "No configuration assets discovered on this machine.")
        );
        return Ok(());
    }

    let selected_files = if args.all {
        discovery
            .files
            .iter()
            .filter(|f| !f.module.is_sensitive())
            .cloned()
            .collect()
    } else {
        let candidate_manifest = build_candidate_manifest(&discovery);
        let filter = match select_items_tree_with_action(
            &candidate_manifest,
            crate::i18n::t!("导出", "Export"),
        )? {
            Some(f) => f,
            None => return Ok(()),
        };
        let files = filter_discovery_files(&discovery, &filter);
        if files.is_empty() {
            println!(
                "{} {}",
                "ℹ️ ".blue(),
                crate::i18n::t!("未选择任何配置项，取消导出。", "No items selected, export cancelled.")
            );
            return Ok(());
        }
        files
    };

    let output_path = match args.output {
        Some(p) => p,
        None if std::io::stdin().is_tty() && std::io::stdout().is_tty() => {
            let default_name = format!(
                "agentsync-export-{}.zip",
                chrono::Utc::now().format("%Y%m%d-%H%M%S")
            );
            let path_str = inquire::Text::new(crate::i18n::t!("导出压缩包路径:", "Export archive path:"))
                .with_default(&default_name)
                .with_help_message(crate::i18n::t!(
                    "按 Enter 直接使用默认文件名，或输入新的保存路径和文件名",
                    "Press Enter to use default filename, or enter custom path and filename"
                ))
                .prompt()
                .map_err(std::io::Error::other)?;

            let trimmed = path_str.trim();
            if trimmed.is_empty() {
                PathBuf::from(default_name)
            } else {
                PathBuf::from(trimmed)
            }
        }
        None => {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            PathBuf::from(format!("agentsync-export-{}.zip", ts))
        }
    };

    export_to_file(&output_path, &discovery, &selected_files)?;
    println!(
        "\n{} {}",
        "🎉 ".green().bold(),
        crate::i18n::t!(
            format!(
                "导出成功！{} 个文件已归档至: {}",
                selected_files.len(),
                output_path.display()
            ),
            format!(
                "Export succeeded! {} files archived to: {}",
                selected_files.len(),
                output_path.display()
            )
        )
    );
    Ok(())
}

fn run_import(args: ImportArgs) -> std::io::Result<()> {
    if !args.file.exists() {
        eprintln!(
            "{} {}: {}",
            "❌ ".red(),
            crate::i18n::t!("文件不存在", "File does not exist"),
            args.file.display()
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            crate::i18n::t!("归档文件不存在", "Archive file does not exist"),
        ));
    }

    let file = File::open(&args.file)?;
    let archive = ZipArchive::new(file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let strategy = if args.force {
        ConflictStrategy::Force
    } else if args.skip_existing {
        ConflictStrategy::Skip
    } else {
        ConflictStrategy::Prompt
    };

    let interactive = !args.all && !args.yes;
    let result = import_archive(archive, strategy, !args.no_backup, interactive, None)?;
    tui::print_import_report(&result);
    Ok(())
}

fn run_send(args: SendArgs) -> std::io::Result<()> {
    let tools_filter = parse_tools_arg(args.tool.as_deref());
    let discovery = scan_configurations(tools_filter.as_deref())?;
    if discovery.files.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!("本机未发现任何配置资产。", "No configuration assets discovered on this machine.")
        );
        return Ok(());
    }

    let selected_files = if args.all {
        discovery
            .files
            .iter()
            .filter(|f| !f.module.is_sensitive())
            .cloned()
            .collect()
    } else {
        let candidate_manifest = build_candidate_manifest(&discovery);
        let filter = match select_items_tree_with_action(
            &candidate_manifest,
            crate::i18n::t!("发送", "Send"),
        )? {
            Some(f) => f,
            None => return Ok(()),
        };
        let files = filter_discovery_files(&discovery, &filter);
        if files.is_empty() {
            println!(
                "{} {}",
                "ℹ️ ".blue(),
                crate::i18n::t!("未选择任何配置项，取消直传。", "No items selected, direct transfer cancelled.")
            );
            return Ok(());
        }
        files
    };

    let archive_bytes = export_to_memory(&discovery, &selected_files)?;
    start_send_server(archive_bytes, args.port, args.ip, args.nopin, args.once)
}

fn run_receive(args: ReceiveArgs) -> std::io::Result<()> {
    let strategy = if args.force {
        ConflictStrategy::Force
    } else if args.yes {
        ConflictStrategy::Skip
    } else {
        ConflictStrategy::Prompt
    };

    let interactive = !args.all && !args.yes;
    let pin_val = if args.nopin {
        None
    } else if let Some(ref p) = args.pin {
        Some(p.clone())
    } else if !args.yes {
        let prompt_msg = crate::i18n::t!(
            "请输入连接 PIN 码 (若服务端开启免密可直接回车留空):",
            "Enter PIN code (leave empty and press Enter if server has disabled PIN):"
        );
        let entered = inquire::Text::new(prompt_msg)
            .prompt()
            .map_err(std::io::Error::other)?;
        let trimmed = entered.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            crate::i18n::t!(
                "缺少 --pin 参数。如需免密连接请指定 --nopin 参数",
                "Missing --pin argument. To connect without PIN, specify --nopin"
            ),
        ));
    };

    let result = receive_and_import(
        &args.addr,
        pin_val.as_deref(),
        strategy,
        !args.no_backup,
        interactive,
        None,
    )?;
    tui::print_import_report(&result);
    Ok(())
}

fn run_backup(args: BackupArgs) -> std::io::Result<()> {
    match args.command {
        BackupCommands::List => {
            let snapshots = list_snapshots()?;
            if snapshots.is_empty() {
                println!(
                    "{} {}",
                    "ℹ️ ".blue(),
                    crate::i18n::t!("暂无历史备份快照。", "No backup snapshots found.")
                );
                return Ok(());
            }
            println!(
                "\n{} {}",
                "🛡️ ".green().bold(),
                crate::i18n::t!("历史快照列表:", "Backup snapshot list:")
            );
            for snap in snapshots {
                println!("  - {}", snap.display());
            }
            println!();
            Ok(())
        }
        BackupCommands::Restore { snapshot_name } => {
            let snapshots = list_snapshots()?;
            let target = snapshots.into_iter().find(|s| {
                s.file_name().map(|n| n.to_string_lossy().to_string()) == Some(snapshot_name.clone())
            });

            if let Some(target_dir) = target {
                let count = restore_snapshot(&target_dir)?;
                println!(
                    "{} {}",
                    "✅ ".green().bold(),
                    crate::i18n::t!(
                        format!("成功从快照恢复 {} 个文件！", count),
                        format!("Successfully restored {} files from snapshot!", count)
                    )
                );
                Ok(())
            } else {
                eprintln!(
                    "{} {}: {}",
                    "❌ ".red(),
                    crate::i18n::t!("未找到快照", "Snapshot not found"),
                    snapshot_name
                );
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    crate::i18n::t!("未找到指定快照", "Specified snapshot not found"),
                ))
            }
        }
    }
}
