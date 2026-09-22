use crate::model::{FileEntry, Manifest, ModuleId, ToolId};
use chrono::Utc;
use colored::Colorize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    Prompt,
    Force,
    Skip,
}

#[derive(Debug, Clone, Default)]
pub struct ImportFilter {
    // (ToolId, ModuleId, Option<String>)
    // If third element is None, all items under that (tool, module) are included.
    pub selected_items: HashSet<(ToolId, ModuleId, Option<String>)>,
}

pub fn file_matches_item(file_entry: &FileEntry, item_name: &str) -> bool {
    let norm_arch = file_entry.archive_path.replace('\\', "/");
    let norm_target = file_entry.target_relative_path.replace('\\', "/");
    let norm_item = item_name.replace('\\', "/");

    match file_entry.module {
        ModuleId::Skills => {
            if let Some(rest) = norm_arch.split("/skills/").nth(1) {
                if let Some(skill_folder) = rest.split('/').next() {
                    return skill_folder.eq_ignore_ascii_case(&norm_item);
                }
            }
        }
        ModuleId::Plugins => {
            if let Some(rest) = norm_arch.split("/plugins/").nth(1) {
                if let Some(plugin_folder) = rest.split('/').next() {
                    return plugin_folder.eq_ignore_ascii_case(&norm_item);
                }
            }
        }
        ModuleId::Hooks => {
            if let Some(rest) = norm_arch.split("/hooks/").nth(1) {
                if rest.eq_ignore_ascii_case(&norm_item) {
                    return true;
                }
            }
        }
        _ => {}
    }

    norm_target.ends_with(&norm_item)
        || norm_arch.ends_with(&norm_item)
        || norm_target.split('/').next_back() == norm_item.split('/').next_back()
}

pub struct ImportResult {
    pub total_files: usize,
    pub written_files: Vec<PathBuf>,
    pub skipped_files: Vec<PathBuf>,
    pub merged_files: Vec<PathBuf>,
    pub backup_dir: Option<PathBuf>,
}

impl ImportResult {
    pub fn empty() -> Self {
        Self {
            total_files: 0,
            written_files: Vec::new(),
            skipped_files: Vec::new(),
            merged_files: Vec::new(),
            backup_dir: None,
        }
    }
}

pub fn read_manifest<R: Read + Seek>(archive: &mut ZipArchive<R>) -> io::Result<Manifest> {
    let mut manifest_file = archive
        .by_name("manifest.json")
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                crate::i18n::t!(
                    format!("归档中未找到 manifest.json: {}", e),
                    format!("manifest.json not found in archive: {}", e)
                ),
            )
        })?;

    let mut content = String::new();
    manifest_file.read_to_string(&mut content)?;
    serde_json::from_str(&content)
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                crate::i18n::t!(
                    format!("manifest.json 解析失败: {}", e),
                    format!("Failed to parse manifest.json: {}", e)
                ),
            )
        })
}

pub fn select_import_items_interactive(
    manifest: &Manifest,
    existing_filter: Option<ImportFilter>,
) -> io::Result<Option<ImportFilter>> {
    if let Some(ref f) = existing_filter {
        if !f.selected_items.is_empty() {
            return Ok(Some(f.clone()));
        }
    }

    crate::tree_select::select_items_tree(manifest)
}

pub fn create_backup_snapshot(files_to_touch: &[PathBuf]) -> io::Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::i18n::t!("未找到用户主目录", "User home directory not found"),
        )
    })?;

    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let backup_dir = home_dir.join(".agentsync").join("backups").join(format!("snapshot-{}", timestamp));
    fs::create_dir_all(&backup_dir)?;

    let mut backed_up_count = 0;

    for file_path in files_to_touch {
        if file_path.exists() && file_path.is_file() {
            if let Ok(rel) = file_path.strip_prefix(&home_dir) {
                let dest = backup_dir.join(rel);
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(file_path, dest)?;
                backed_up_count += 1;
            }
        }
    }

    let meta = serde_json::json!({
        "timestamp": timestamp.to_string(),
        "backed_up_files_count": backed_up_count,
    });
    fs::write(backup_dir.join("snapshot_meta.json"), serde_json::to_string_pretty(&meta)?)?;

    Ok(backup_dir)
}

pub fn is_file_matching_filter(file_entry: &FileEntry, filter: &ImportFilter) -> bool {
    if filter.selected_items.contains(&(file_entry.tool, file_entry.module, None)) {
        return true;
    }

    for (sel_tool, sel_mod, sel_item) in &filter.selected_items {
        if *sel_tool == file_entry.tool && *sel_mod == file_entry.module {
            if let Some(ref item_name) = sel_item {
                if file_matches_item(file_entry, item_name) {
                    return true;
                }
            }
        }
    }

    false
}

pub fn import_archive<R: Read + Seek>(
    mut archive: ZipArchive<R>,
    strategy: ConflictStrategy,
    do_backup: bool,
    interactive: bool,
    filter: Option<ImportFilter>,
) -> io::Result<ImportResult> {
    let manifest = read_manifest(&mut archive)?;
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::i18n::t!("未能获取用户主目录", "Failed to get user home directory"),
        )
    })?;

    // 交互模式下若未完整指定过滤规则，弹出模块与子项细选向导
    let active_filter = if interactive {
        match select_import_items_interactive(&manifest, filter)? {
            Some(f) => Some(f),
            None => return Ok(ImportResult::empty()),
        }
    } else {
        filter
    };

    // 筛选出符合条件的目标文件
    let files_to_import: Vec<&FileEntry> = manifest
        .files
        .iter()
        .filter(|file_entry| {
            if let Some(ref f) = active_filter {
                is_file_matching_filter(file_entry, f)
            } else {
                true
            }
        })
        .collect();

    if files_to_import.is_empty() {
        println!(
            "{} {}",
            "ℹ️ ".blue(),
            crate::i18n::t!(
                "筛选后无任何匹配的配置项需要导入。",
                "No matching configuration items found to import after filtering."
            )
        );
        return Ok(ImportResult::empty());
    }

    if interactive {
        let confirm_msg = crate::i18n::t!(
            format!("确认将选中的 {} 个文件应用导入到当前系统？", files_to_import.len()),
            format!("Confirm applying and importing {} selected files to current system?", files_to_import.len())
        );
        let confirmed = inquire::Confirm::new(&confirm_msg)
            .with_default(true)
            .prompt()
            .unwrap_or(false);
        if !confirmed {
            println!(
                "{} {}",
                "❌ ".yellow(),
                crate::i18n::t!("操作已取消。", "Operation cancelled.")
            );
            return Ok(ImportResult::empty());
        }
    }

    // 收集所有被选中文件的目标路径
    let mut target_paths = Vec::new();
    for file_entry in &files_to_import {
        let target_path = home_dir.join(&file_entry.target_relative_path);
        target_paths.push(target_path);
    }

    // 执行自动备份
    let backup_dir = if do_backup {
        let b_dir = create_backup_snapshot(&target_paths)?;
        println!(
            "{} {}",
            "🛡️ ".green(),
            crate::i18n::t!(
                format!("已创建备份快照: {}", b_dir.display()),
                format!("Created backup snapshot: {}", b_dir.display())
            )
        );
        Some(b_dir)
    } else {
        None
    };

    let mut written_files = Vec::new();
    let mut skipped_files = Vec::new();
    let mut merged_files = Vec::new();

    for file_entry in files_to_import {
        let target_path = home_dir.join(&file_entry.target_relative_path);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut zip_file = match archive.by_name(&file_entry.archive_path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let is_mcp_config = file_entry.archive_path.contains("mcp_config.json")
            || file_entry.archive_path.contains("mcp.json");

        if target_path.exists() && is_mcp_config {
            let mut incoming_bytes = Vec::new();
            zip_file.read_to_end(&mut incoming_bytes)?;

            match merge_mcp_config(&target_path, &incoming_bytes) {
                Ok(merged_bytes) => {
                    fs::write(&target_path, merged_bytes)?;
                    merged_files.push(target_path.clone());
                    continue;
                }
                Err(e) => {
                    eprintln!(
                        "{} {}",
                        "⚠️ ".yellow(),
                        crate::i18n::t!(
                            format!("MCP配置合并失败 ({})，回退到冲突策略处理", e),
                            format!("MCP config merge failed ({}), falling back to conflict strategy", e)
                        )
                    );
                }
            }
        }

        if target_path.exists() {
            let resolve = match strategy {
                ConflictStrategy::Force => true,
                ConflictStrategy::Skip => false,
                ConflictStrategy::Prompt => {
                    let prompt_text = crate::i18n::t!(
                        format!("文件已存在: {}，是否覆盖？", target_path.display()),
                        format!("File already exists: {}, overwrite?", target_path.display())
                    );
                    inquire::Confirm::new(&prompt_text)
                        .with_default(false)
                        .prompt()
                        .unwrap_or(false)
                }
            };

            if !resolve {
                skipped_files.push(target_path);
                continue;
            }
        }

        let mut out = fs::File::create(&target_path)?;
        io::copy(&mut zip_file, &mut out)?;
        written_files.push(target_path);
    }

    Ok(ImportResult {
        total_files: written_files.len() + skipped_files.len() + merged_files.len(),
        written_files,
        skipped_files,
        merged_files,
        backup_dir,
    })
}

fn merge_mcp_config(local_path: &Path, incoming_bytes: &[u8]) -> io::Result<Vec<u8>> {
    let local_bytes = fs::read(local_path)?;
    let mut local_json: Value = serde_json::from_slice(&local_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let incoming_json: Value = serde_json::from_slice(incoming_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if let (Some(local_obj), Some(incoming_obj)) = (local_json.as_object_mut(), incoming_json.as_object()) {
        if let Some(incoming_servers) = incoming_obj.get("mcpServers").and_then(|v| v.as_object()) {
            let local_servers = local_obj
                .entry("mcpServers")
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        crate::i18n::t!("mcpServers 格式无效", "Invalid format for mcpServers"),
                    )
                })?;

            for (server_name, server_config) in incoming_servers {
                local_servers.insert(server_name.clone(), server_config.clone());
            }
        }
    }

    serde_json::to_vec_pretty(&local_json)
        .map_err(io::Error::other)
}

pub fn list_snapshots() -> io::Result<Vec<PathBuf>> {
    let mut dirs_to_check = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs_to_check.push(home.join(".agentsync").join("backups"));
        dirs_to_check.push(home.join(".aitrans").join("backups"));
        dirs_to_check.push(home.join(".gemini").join("backups"));
    }

    let mut snapshots = Vec::new();
    for backup_dir in dirs_to_check {
        if backup_dir.exists() && backup_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(backup_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && entry.file_name().to_string_lossy().starts_with("snapshot-")
                        && !snapshots.iter().any(|p: &PathBuf| p.file_name() == path.file_name()) {
                            snapshots.push(path);
                        }
                }
            }
        }
    }
    snapshots.sort();
    snapshots.reverse();
    Ok(snapshots)
}

pub fn restore_snapshot(snapshot_dir: &Path) -> io::Result<usize> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::i18n::t!("未找到用户主目录", "User home directory not found"),
        )
    })?;

    let mut restored_count = 0;
    for entry in walkdir::WalkDir::new(snapshot_dir).into_iter().flatten() {
        let path = entry.path();
        if path.is_file() && entry.file_name() != "snapshot_meta.json" {
            if let Ok(rel) = path.strip_prefix(snapshot_dir) {
                let target = home_dir.join(rel);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(path, target)?;
                restored_count += 1;
            }
        }
    }
    Ok(restored_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_matches_item() {
        let skill_file = FileEntry {
            archive_path: "data/antigravity/skills/cm/SKILL.md".into(),
            target_relative_path: ".gemini/config/skills/cm/SKILL.md".into(),
            sha256: "dummy".into(),
            size_bytes: 10,
            tool: ToolId::Antigravity,
            module: ModuleId::Skills,
        };
        assert!(file_matches_item(&skill_file, "cm"));
        assert!(!file_matches_item(&skill_file, "faq"));

        let hook_file = FileEntry {
            archive_path: "data/claude/hooks/pre-commit.ps1".into(),
            target_relative_path: ".claude/hooks/pre-commit.ps1".into(),
            sha256: "dummy".into(),
            size_bytes: 20,
            tool: ToolId::Claude,
            module: ModuleId::Hooks,
        };
        assert!(file_matches_item(&hook_file, "pre-commit.ps1"));
        assert!(!file_matches_item(&hook_file, "post-checkout.sh"));

        let settings_file = FileEntry {
            archive_path: "data/codex/settings/config.toml".into(),
            target_relative_path: ".codex/config.toml".into(),
            sha256: "dummy".into(),
            size_bytes: 30,
            tool: ToolId::Codex,
            module: ModuleId::Settings,
        };
        assert!(file_matches_item(&settings_file, "config.toml"));
    }
}
