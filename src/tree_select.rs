use crate::importer::ImportFilter;
use crate::model::{Manifest, ModuleId, ToolId, ToolManifest};
use colored::Colorize;
use crossterm::{
    cursor,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute, queue,
    terminal::{self, ClearType},
    tty::IsTty,
};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    PartiallyChecked,
}

#[derive(Debug, Clone)]
pub struct TreeItem {
    #[allow(dead_code)]
    pub id: String,
    pub label: String,
    pub description: String,
    pub indent: usize,
    pub is_parent: bool,
    pub is_sensitive: bool,
    pub parent_idx: Option<usize>,
    pub child_indices: Vec<usize>,
    pub state: CheckState,
    pub tool_id: ToolId,
    pub module_id: Option<ModuleId>,
    pub sub_item_name: Option<String>,
    pub local_path: Option<PathBuf>,
}

fn char_width(c: char) -> usize {
    if c == '\t' {
        4
    } else if c.is_ascii() {
        1
    } else {
        2
    }
}

fn str_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn truncate_to_width(s: &str, max_width: usize) -> String {
    let mut current_width = 0;
    let mut result = String::new();
    for c in s.chars() {
        let w = char_width(c);
        if current_width + w > max_width {
            break;
        }
        result.push(c);
        current_width += w;
    }
    result
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture
        )?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            cursor::Show,
            terminal::LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = terminal::disable_raw_mode();
    }
}

fn normalize_path_for_windows(path: &Path) -> String {
    let mut s = path.to_string_lossy().to_string();
    s = s.replace('/', "\\");
    if let Some(stripped) = s.strip_prefix("\\\\?\\") {
        s = stripped.to_string();
    }
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    s
}

pub fn open_in_system(path: &Path) -> io::Result<()> {
    let mut target = path.to_path_buf();

    // 如果路径尚不存在，回溯到最接近的现存祖先目录
    while !target.exists() {
        if let Some(parent) = target.parent() {
            target = parent.to_path_buf();
        } else {
            break;
        }
    }

    if !target.exists() {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let clean_path = normalize_path_for_windows(&target);

        if target.is_file() {
            std::process::Command::new("explorer")
                .raw_arg(format!("/select,\"{}\"", clean_path))
                .spawn()?;
        } else {
            std::process::Command::new("explorer")
                .raw_arg(format!("\"{}\"", clean_path))
                .spawn()?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = std::process::Command::new("open");
        if target.is_file() {
            cmd.arg("-R").arg(&target);
        } else {
            cmd.arg(&target);
        }
        cmd.spawn()?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&target).spawn()?;
    }

    Ok(())
}

fn resolve_item_path(
    manifest: &Manifest,
    home_dir: Option<&Path>,
    tool: &ToolManifest,
    module_id: ModuleId,
    sub_item: Option<&str>,
) -> Option<PathBuf> {
    let home = home_dir?;
    let tool_base_rel = tool.base_dir.replace('/', "\\");
    let tool_base = home.join(tool_base_rel);

    if let Some(item_name) = sub_item {
        let clean_item = item_name.replace('/', "\\");

        // 1. 在 manifest.files 中优先查找
        for file in &manifest.files {
            if file.tool == tool.id && file.module == module_id {
                let full_rel = file.target_relative_path.replace('/', "\\");
                let full_path = home.join(&full_rel);

                let skills_pattern = format!("\\skills\\{}\\", clean_item);
                let plugins_pattern = format!("\\plugins\\{}\\", clean_item);
                let hooks_pattern = format!("\\hooks\\{}", clean_item);

                if full_rel.contains(&skills_pattern) || full_rel.contains(&plugins_pattern) {
                    let mut p = full_path.as_path();
                    while let Some(parent) = p.parent() {
                        if parent.file_name().and_then(|n| n.to_str()) == Some(item_name) {
                            return Some(parent.to_path_buf());
                        }
                        p = parent;
                    }
                }

                if full_rel.contains(&hooks_pattern)
                    || full_rel.ends_with(&clean_item)
                    || full_rel.ends_with(item_name)
                {
                    return Some(full_path);
                }
            }
        }

        // 2. 物理目录结构探测
        match module_id {
            ModuleId::Skills => {
                let p1 = tool_base.join("config").join("skills").join(&clean_item);
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("skills").join(&clean_item);
                if p2.exists() {
                    return Some(p2);
                }
                Some(p1)
            }
            ModuleId::Plugins => {
                let p1 = tool_base.join("config").join("plugins").join(&clean_item);
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("plugins").join(&clean_item);
                if p2.exists() {
                    return Some(p2);
                }
                Some(p1)
            }
            ModuleId::Hooks => {
                let p = tool_base.join("hooks").join(&clean_item);
                Some(p)
            }
            _ => {
                let p1 = tool_base.join(&clean_item);
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("config").join(&clean_item);
                if p2.exists() {
                    return Some(p2);
                }
                Some(p1)
            }
        }
    } else {
        // 模块父节点
        match module_id {
            ModuleId::Skills => {
                let p1 = tool_base.join("config").join("skills");
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("skills");
                if p2.exists() {
                    return Some(p2);
                }
                Some(p1)
            }
            ModuleId::Plugins => {
                let p1 = tool_base.join("config").join("plugins");
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("plugins");
                if p2.exists() {
                    return Some(p2);
                }
                Some(p1)
            }
            ModuleId::Hooks => Some(tool_base.join("hooks")),
            ModuleId::Mcp => {
                let p1 = tool_base.join("config").join("mcp_config.json");
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = tool_base.join("mcp_config.json");
                if p2.exists() {
                    return Some(p2);
                }
                let p3 = tool_base.join(".mcp.json");
                if p3.exists() {
                    return Some(p3);
                }
                let p4 = tool_base.join("mcp.json");
                if p4.exists() {
                    return Some(p4);
                }
                Some(tool_base)
            }
            ModuleId::Rules => {
                for cand in &["GEMINI.md", "CLAUDE.md", "AGENTS.md", "OPENCODE.md"] {
                    let p = tool_base.join(cand);
                    if p.exists() {
                        return Some(p);
                    }
                }
                Some(tool_base)
            }
            ModuleId::Settings => {
                for cand in &[
                    "config/config.json",
                    "antigravity-cli/settings.json",
                    "settings.json",
                    "config.json",
                    "config.toml",
                ] {
                    let p = tool_base.join(cand.replace('/', "\\"));
                    if p.exists() {
                        return Some(p);
                    }
                }
                Some(tool_base)
            }
            ModuleId::Credentials => {
                for cand in &[
                    "google_accounts.json",
                    "oauth_creds.json",
                    "auth.json",
                    "token.json",
                    ".credentials.json",
                ] {
                    let p = tool_base.join(cand);
                    if p.exists() {
                        return Some(p);
                    }
                }
                Some(tool_base)
            }
            ModuleId::State => {
                for cand in &[
                    "state.json",
                    "antigravity-cli/history.jsonl",
                    "history.jsonl",
                ] {
                    let p = tool_base.join(cand.replace('/', "\\"));
                    if p.exists() {
                        return Some(p);
                    }
                }
                Some(tool_base)
            }
        }
    }
}

pub fn build_tree_items(manifest: &Manifest) -> Vec<TreeItem> {
    let mut items = Vec::new();
    let home_dir = dirs::home_dir();

    for tool in &manifest.tools {
        let tool_idx = items.len();
        let tool_base_rel = tool.base_dir.replace('/', "\\");
        let tool_path = home_dir.as_ref().map(|h| h.join(tool_base_rel));

        // Level 0: AI 工具根节点
        let mod_count_label = if crate::i18n::is_zh() {
            format!("共 {} 模块", tool.modules.len())
        } else {
            format!("{} modules", tool.modules.len())
        };
        let desc_label = if crate::i18n::is_zh() {
            format!("{} 配置文件目录与资产", tool.name)
        } else {
            format!("{} configuration directory and assets", tool.name)
        };

        items.push(TreeItem {
            id: format!("tool:{}", tool.id.as_str()),
            label: format!("{} ({})", tool.name, mod_count_label),
            description: desc_label,
            indent: 0,
            is_parent: true,
            is_sensitive: false,
            parent_idx: None,
            child_indices: Vec::new(),
            state: CheckState::Checked,
            tool_id: tool.id,
            module_id: None,
            sub_item_name: None,
            local_path: tool_path,
        });

        let mut tool_children = Vec::new();

        for module in &tool.modules {
            let is_sensitive = module.id.is_sensitive();
            let default_state = if module.id.default_selected() {
                CheckState::Checked
            } else {
                CheckState::Unchecked
            };

            let sensitive_tag = crate::t!("[敏感]", "[Sensitive]").yellow().bold();
            let base_title = if is_sensitive {
                format!("{} {}", module.id.display_name(), sensitive_tag)
            } else {
                module.id.display_name().to_string()
            };

            let is_bundle = matches!(module.id, ModuleId::Skills | ModuleId::Plugins);
            let has_children = (is_bundle && !module.items.is_empty()) || module.items.len() > 1;

            if has_children {
                // Level 1: 模块父节点
                let mod_idx = items.len();
                tool_children.push(mod_idx);
                let mod_path = resolve_item_path(manifest, home_dir.as_deref(), tool, module.id, None);
                let items_count_label = if crate::i18n::is_zh() {
                    format!("共 {} 项", module.items.len())
                } else {
                    format!("{} items", module.items.len())
                };

                items.push(TreeItem {
                    id: format!("{}:mod:{}", tool.id.as_str(), module.id.as_str()),
                    label: format!("{} ({})", base_title, items_count_label),
                    description: crate::descriptions::get_module_description(module.id).to_string(),
                    indent: 1,
                    is_parent: true,
                    is_sensitive,
                    parent_idx: Some(tool_idx),
                    child_indices: Vec::new(),
                    state: default_state.clone(),
                    tool_id: tool.id,
                    module_id: Some(module.id),
                    sub_item_name: None,
                    local_path: mod_path,
                });

                let mut mod_children = Vec::new();
                for item_name in &module.items {
                    // Level 2: 具体资产细项
                    let child_idx = items.len();
                    mod_children.push(child_idx);

                    let desc = crate::descriptions::resolve_item_description(
                        module.id,
                        item_name,
                        Some(&module.descriptions),
                    );
                    let item_path = resolve_item_path(
                        manifest,
                        home_dir.as_deref(),
                        tool,
                        module.id,
                        Some(item_name),
                    );

                    items.push(TreeItem {
                        id: format!("{}:{}:{}", tool.id.as_str(), module.id.as_str(), item_name),
                        label: item_name.clone(),
                        description: desc,
                        indent: 2,
                        is_parent: false,
                        is_sensitive,
                        parent_idx: Some(mod_idx),
                        child_indices: Vec::new(),
                        state: default_state.clone(),
                        tool_id: tool.id,
                        module_id: Some(module.id),
                        sub_item_name: Some(item_name.clone()),
                        local_path: item_path,
                    });
                }
                items[mod_idx].child_indices = mod_children;
            } else if module.items.len() == 1 {
                // Level 1: 单项模块（作为叶子）
                let mod_idx = items.len();
                tool_children.push(mod_idx);

                let item_name = &module.items[0];
                let label = format!("{} ({})", base_title, item_name);
                let desc = crate::descriptions::resolve_item_description(
                    module.id,
                    item_name,
                    Some(&module.descriptions),
                );
                let item_path = resolve_item_path(
                    manifest,
                    home_dir.as_deref(),
                    tool,
                    module.id,
                    Some(item_name),
                );

                items.push(TreeItem {
                    id: format!("{}:mod:{}", tool.id.as_str(), module.id.as_str()),
                    label,
                    description: desc,
                    indent: 1,
                    is_parent: false,
                    is_sensitive,
                    parent_idx: Some(tool_idx),
                    child_indices: Vec::new(),
                    state: default_state,
                    tool_id: tool.id,
                    module_id: Some(module.id),
                    sub_item_name: Some(item_name.clone()),
                    local_path: item_path,
                });
            } else {
                // Level 1: 无细项模块（作为叶子）
                let mod_idx = items.len();
                tool_children.push(mod_idx);
                let mod_path = resolve_item_path(manifest, home_dir.as_deref(), tool, module.id, None);

                items.push(TreeItem {
                    id: format!("{}:mod:{}", tool.id.as_str(), module.id.as_str()),
                    label: base_title.to_string(),
                    description: crate::descriptions::get_module_description(module.id).to_string(),
                    indent: 1,
                    is_parent: false,
                    is_sensitive,
                    parent_idx: Some(tool_idx),
                    child_indices: Vec::new(),
                    state: default_state,
                    tool_id: tool.id,
                    module_id: Some(module.id),
                    sub_item_name: None,
                    local_path: mod_path,
                });
            }
        }

        items[tool_idx].child_indices = tool_children;
    }

    // 从叶子向上逐层刷新父节点的初始 CheckState
    refresh_all_parent_states(&mut items);

    items
}

fn set_node_state_recursive(items: &mut [TreeItem], idx: usize, new_state: CheckState) {
    items[idx].state = new_state.clone();
    for c_idx in items[idx].child_indices.clone() {
        set_node_state_recursive(items, c_idx, new_state.clone());
    }
}

fn update_ancestors_state(items: &mut [TreeItem], mut parent_idx_opt: Option<usize>) {
    while let Some(parent_idx) = parent_idx_opt {
        let child_indices = items[parent_idx].child_indices.clone();
        if !child_indices.is_empty() {
            let total = child_indices.len();
            let checked_count = child_indices
                .iter()
                .filter(|&&c| items[c].state == CheckState::Checked)
                .count();
            let unchecked_count = child_indices
                .iter()
                .filter(|&&c| items[c].state == CheckState::Unchecked)
                .count();

            if checked_count == total {
                items[parent_idx].state = CheckState::Checked;
            } else if unchecked_count == total {
                items[parent_idx].state = CheckState::Unchecked;
            } else {
                items[parent_idx].state = CheckState::PartiallyChecked;
            }
        }
        parent_idx_opt = items[parent_idx].parent_idx;
    }
}

fn refresh_all_parent_states(items: &mut [TreeItem]) {
    for idx in (0..items.len()).rev() {
        if items[idx].is_parent {
            let child_indices = items[idx].child_indices.clone();
            if !child_indices.is_empty() {
                let total = child_indices.len();
                let checked_count = child_indices
                    .iter()
                    .filter(|&&c| items[c].state == CheckState::Checked)
                    .count();
                let unchecked_count = child_indices
                    .iter()
                    .filter(|&&c| items[c].state == CheckState::Unchecked)
                    .count();

                if checked_count == total {
                    items[idx].state = CheckState::Checked;
                } else if unchecked_count == total {
                    items[idx].state = CheckState::Unchecked;
                } else {
                    items[idx].state = CheckState::PartiallyChecked;
                }
            }
        }
    }
}

fn toggle_item(items: &mut [TreeItem], idx: usize) {
    if idx >= items.len() {
        return;
    }

    let target_state = match items[idx].state {
        CheckState::Checked => CheckState::Unchecked,
        CheckState::Unchecked | CheckState::PartiallyChecked => CheckState::Checked,
    };

    set_node_state_recursive(items, idx, target_state);
    update_ancestors_state(items, items[idx].parent_idx);
}

fn select_all(items: &mut [TreeItem]) {
    for item in items.iter_mut() {
        if !item.is_parent && !item.is_sensitive {
            item.state = CheckState::Checked;
        }
    }
    refresh_all_parent_states(items);
}

fn deselect_all(items: &mut [TreeItem]) {
    for item in items.iter_mut() {
        item.state = CheckState::Unchecked;
    }
}

fn extract_filter_from_items(items: &[TreeItem]) -> ImportFilter {
    let mut filter = ImportFilter::default();

    for item in items {
        if !item.is_parent && item.state == CheckState::Checked {
            if let Some(module) = item.module_id {
                filter.selected_items.insert((item.tool_id, module, item.sub_item_name.clone()));
            }
        }
    }

    filter
}

pub fn select_items_tree(manifest: &Manifest) -> io::Result<Option<ImportFilter>> {
    select_items_tree_with_action(manifest, crate::i18n::t!("导入", "import"))
}

pub fn select_items_tree_with_action(
    manifest: &Manifest,
    action_verb: &str,
) -> io::Result<Option<ImportFilter>> {
    let mut items = build_tree_items(manifest);
    if items.is_empty() {
        println!(
            "{} {}",
            "⚠️ ".yellow(),
            crate::i18n::t!("未包含任何配置资产。", "No configuration assets found.")
        );
        return Ok(None);
    }

    if !io::stdin().is_tty() || !io::stdout().is_tty() {
        return Ok(Some(extract_filter_from_items(&items)));
    }

    let _guard = RawModeGuard::enter()?;
    let mut stdout = BufWriter::new(io::stdout());

    let mut cursor_idx: usize = 0;
    let mut window_start: usize = 0;
    let mut hovered_idx: Option<usize> = None;
    let mut status_feedback: Option<String> = None;

    loop {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let page_size = (rows.saturating_sub(5) as usize).clamp(4, 30).min(items.len());

        if cursor_idx < window_start {
            window_start = cursor_idx;
        } else if cursor_idx >= window_start + page_size {
            window_start = cursor_idx - page_size + 1;
        }

        let mut lines: Vec<String> = Vec::new();

        let title_line = if crate::i18n::is_zh() {
            format!(
                "{} 请选择待{}的 AI 工具与配置项目 (↑/↓ 移动, 空格 切换勾选, a 全选, n 全不选, Enter 确认):",
                "?".green().bold(),
                action_verb
            )
        } else {
            let action_en = match action_verb {
                "导出" | "Export" | "export" => "export",
                "导入" | "Import" | "import" => "import",
                "发送" | "Send" | "send" => "send",
                _ => action_verb,
            };
            format!(
                "{} Select AI tools and configurations to {} (↑/↓ Move, Space Toggle, a Select all, n Deselect all, Enter Confirm):",
                "?".green().bold(),
                action_en
            )
        };
        lines.push(title_line);

        if window_start > 0 {
            let overflow_up = if crate::i18n::is_zh() {
                format!("    ▲ (上方还有 {} 项)", window_start)
            } else {
                format!("    ▲ ({} more items above)", window_start)
            };
            lines.push(overflow_up.dimmed().to_string());
        } else {
            lines.push("".to_string());
        }

        let window_end = (window_start + page_size).min(items.len());
        #[allow(clippy::needless_range_loop)]
        for idx in window_start..window_end {
            let item = &items[idx];
            let is_cursor = idx == cursor_idx;
            let is_hovered = Some(idx) == hovered_idx;

            let cursor_sym = if is_cursor {
                ">".cyan().bold().to_string()
            } else {
                " ".to_string()
            };

            let check_sym = match item.state {
                CheckState::Checked => "[x]".green().bold().to_string(),
                CheckState::PartiallyChecked => "[-]".yellow().bold().to_string(),
                CheckState::Unchecked => "[ ]".dimmed().to_string(),
            };

            let indent_str = match item.indent {
                0 => "",
                1 => "  ",
                _ => "    ",
            };

            let available_width = (cols as usize).saturating_sub(indent_str.len() + 10);
            let label_w = str_width(&item.label);

            let row_text = if !item.description.is_empty() && available_width > label_w + 6 {
                let desc_budget = available_width.saturating_sub(label_w + 4);
                let desc_w = str_width(&item.description);
                let desc_display = if desc_w > desc_budget && desc_budget > 6 {
                    format!("{}...", truncate_to_width(&item.description, desc_budget.saturating_sub(3)))
                } else {
                    item.description.clone()
                };

                if is_cursor && is_hovered {
                    format!(
                        "{} {}",
                        item.label.cyan().bold().underline(),
                        format!("- {}", desc_display).cyan().underline()
                    )
                } else if is_cursor {
                    format!("{} {}", item.label.cyan().bold(), format!("- {}", desc_display).cyan())
                } else if is_hovered {
                    if item.indent == 0 {
                        format!(
                            "{} {}",
                            item.label.bold().underline(),
                            format!("- {}", desc_display).dimmed().underline()
                        )
                    } else {
                        format!(
                            "{} {}",
                            item.label.underline(),
                            format!("- {}", desc_display).dimmed().underline()
                        )
                    }
                } else if item.indent == 0 {
                    format!("{} {}", item.label.bold(), format!("- {}", desc_display).dimmed())
                } else {
                    format!("{} {}", item.label, format!("- {}", desc_display).dimmed())
                }
            } else if is_cursor && is_hovered {
                truncate_to_width(&item.label, available_width)
                    .cyan()
                    .bold()
                    .underline()
                    .to_string()
            } else if is_cursor {
                truncate_to_width(&item.label, available_width)
                    .cyan()
                    .bold()
                    .to_string()
            } else if is_hovered {
                if item.indent == 0 {
                    truncate_to_width(&item.label, available_width)
                        .bold()
                        .underline()
                        .to_string()
                } else {
                    truncate_to_width(&item.label, available_width)
                        .underline()
                        .to_string()
                }
            } else {
                truncate_to_width(&item.label, available_width)
            };

            lines.push(format!("{} {}{} {}", cursor_sym, indent_str, check_sym, row_text));
        }

        if window_end < items.len() {
            let overflow_down = if crate::i18n::is_zh() {
                format!("    ▼ (下方还有 {} 项)", items.len() - window_end)
            } else {
                format!("    ▼ ({} more items below)", items.len() - window_end)
            };
            lines.push(overflow_down.dimmed().to_string());
        } else {
            lines.push("".to_string());
        }

        let status_line = if let Some(ref msg) = status_feedback {
            format!("{} {}", "📂".blue(), msg.yellow())
        } else {
            let selected_count = items
                .iter()
                .filter(|it| !it.is_parent && it.state == CheckState::Checked)
                .count();
            if crate::i18n::is_zh() {
                format!(
                    "{} 已选 {} 项 | [空格/单击] 切换 | [Ctrl+单击/o] 打开对应路径 | [Enter] 确认{} | [Esc/q] 取消",
                    "ℹ️".blue(),
                    selected_count.to_string().green().bold(),
                    action_verb
                )
            } else {
                let action_en = match action_verb {
                    "导出" | "Export" | "export" => "Export",
                    "导入" | "Import" | "import" => "Import",
                    "发送" | "Send" | "send" => "Send",
                    _ => action_verb,
                };
                format!(
                    "{} {} items selected | [Space/Click] Toggle | [Ctrl+Click/o] Open path | [Enter] Confirm {} | [Esc/q] Cancel",
                    "ℹ️".blue(),
                    selected_count.to_string().green().bold(),
                    action_en
                )
            }
        };
        lines.push(status_line);

        queue!(stdout, cursor::MoveTo(0, 0))?;
        for (i, line) in lines.iter().enumerate() {
            queue!(stdout, terminal::Clear(ClearType::CurrentLine))?;
            write!(stdout, "{}", line)?;
            if i + 1 < lines.len() {
                queue!(stdout, cursor::MoveToNextLine(1))?;
            }
        }
        queue!(stdout, terminal::Clear(ClearType::FromCursorDown))?;
        stdout.flush()?;

        match event::read()? {
            Event::Key(KeyEvent { code, modifiers, .. }) => {
                status_feedback = None;
                match code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        cursor_idx = cursor_idx.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if cursor_idx + 1 < items.len() {
                            cursor_idx += 1;
                        }
                    }
                    KeyCode::PageUp => {
                        cursor_idx = cursor_idx.saturating_sub(page_size);
                    }
                    KeyCode::PageDown => {
                        cursor_idx = (cursor_idx + page_size).min(items.len().saturating_sub(1));
                    }
                    KeyCode::Home => {
                        cursor_idx = 0;
                    }
                    KeyCode::End => {
                        cursor_idx = items.len().saturating_sub(1);
                    }
                    KeyCode::Char(' ') => {
                        toggle_item(&mut items, cursor_idx);
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Right => {
                        select_all(&mut items);
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Left => {
                        deselect_all(&mut items);
                    }
                    KeyCode::Char('o') | KeyCode::Char('O') => {
                        if let Some(ref path) = items[cursor_idx].local_path {
                            let _ = open_in_system(path);
                            status_feedback = Some(crate::i18n::t!(
                                format!("已在系统资源管理器中打开: {}", path.display()),
                                format!("Opened in system file manager: {}", path.display())
                            ));
                        } else {
                            status_feedback = Some(
                                crate::i18n::t!(
                                    "当前项无对应的本地路径",
                                    "Current item has no corresponding local path"
                                )
                                .to_string(),
                            );
                        }
                    }
                    KeyCode::Enter => {
                        let filter = extract_filter_from_items(&items);
                        return Ok(Some(filter));
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        return Ok(None);
                    }
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None);
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse_event) => {
                let list_len = window_end.saturating_sub(window_start);
                match mouse_event.kind {
                    MouseEventKind::Moved => {
                        let new_hovered =
                            if mouse_event.row >= 2 && (mouse_event.row as usize) < 2 + list_len {
                                let idx = window_start + (mouse_event.row as usize - 2);
                                if idx < items.len() {
                                    Some(idx)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                        hovered_idx = new_hovered;
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if mouse_event.row >= 2 && (mouse_event.row as usize) < 2 + list_len {
                            let clicked_idx = window_start + (mouse_event.row as usize - 2);
                            if clicked_idx < items.len() {
                                cursor_idx = clicked_idx;
                                if mouse_event.modifiers.contains(KeyModifiers::CONTROL) {
                                    if let Some(ref path) = items[clicked_idx].local_path {
                                        let _ = open_in_system(path);
                                        status_feedback = Some(crate::i18n::t!(
                                            format!("已在系统资源管理器中打开: {}", path.display()),
                                            format!("Opened in system file manager: {}", path.display())
                                        ));
                                    } else {
                                        status_feedback = Some(
                                            crate::i18n::t!(
                                                "当前项无对应的本地路径",
                                                "Current item has no corresponding local path"
                                            )
                                            .to_string(),
                                        );
                                    }
                                } else {
                                    toggle_item(&mut items, clicked_idx);
                                    status_feedback = None;
                                }
                            }
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        cursor_idx = cursor_idx.saturating_sub(1);
                    }
                    MouseEventKind::ScrollDown => {
                        if cursor_idx + 1 < items.len() {
                            cursor_idx += 1;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModuleSummary, ToolManifest};

    fn make_test_manifest() -> Manifest {
        let mut descriptions = std::collections::HashMap::new();
        descriptions.insert("cm".into(), "Git commit tool".into());
        descriptions.insert("faq".into(), "FAQ answers".into());

        let skill_module = ModuleSummary {
            id: ModuleId::Skills,
            name: "🎨 自定义技能".into(),
            count: 2,
            items: vec!["cm".into(), "faq".into()],
            descriptions,
        };

        let cred_module = ModuleSummary {
            id: ModuleId::Credentials,
            name: "🔑 认证凭据".into(),
            count: 1,
            items: vec!["token.json".into()],
            descriptions: std::collections::HashMap::new(),
        };

        let antigravity_tool = ToolManifest {
            id: ToolId::Antigravity,
            name: "🤖 Google Antigravity".into(),
            base_dir: ".gemini".into(),
            modules: vec![skill_module.clone(), cred_module.clone()],
        };

        let claude_tool = ToolManifest {
            id: ToolId::Claude,
            name: "🧠 Anthropic Claude Code".into(),
            base_dir: ".claude".into(),
            modules: vec![skill_module],
        };

        Manifest {
            version: "2.0.0".into(),
            generated_at: chrono::Utc::now(),
            source_platform: crate::model::PlatformInfo {
                os: "windows".into(),
                arch: "x86_64".into(),
                hostname: "test-pc".into(),
            },
            security: crate::model::SecurityInfo {
                contains_credentials: true,
                contains_state: false,
                encrypted: false,
            },
            tools: vec![antigravity_tool, claude_tool],
            files: Vec::new(),
        }
    }

    #[test]
    fn test_tree_construction_with_tools_as_roots() {
        let manifest = make_test_manifest();
        let items = build_tree_items(&manifest);

        // 根节点（Level 0）必须是工具
        assert_eq!(items[0].indent, 0);
        assert_eq!(items[0].tool_id, ToolId::Antigravity);
        assert!(items[0].is_parent);
        assert!(items[0].label.contains("Antigravity"));

        // 子节点（Level 1）是模块
        let child1_idx = items[0].child_indices[0];
        assert_eq!(items[child1_idx].indent, 1);
        assert_eq!(items[child1_idx].module_id, Some(ModuleId::Skills));
        assert!(items[child1_idx].is_parent);

        // 叶子节点（Level 2）是具体项
        let leaf1_idx = items[child1_idx].child_indices[0];
        assert_eq!(items[leaf1_idx].indent, 2);
        assert_eq!(items[leaf1_idx].label, "cm");
        assert!(!items[leaf1_idx].is_parent);
    }

    #[test]
    fn test_toggle_tool_cascades_to_all_children() {
        let manifest = make_test_manifest();
        let mut items = build_tree_items(&manifest);

        // 点击工具根节点 (Antigravity)，从 PartiallyChecked 变为 Checked
        toggle_item(&mut items, 0);
        assert_eq!(items[0].state, CheckState::Checked);
        for &c_idx in &items[0].child_indices {
            assert_eq!(items[c_idx].state, CheckState::Checked);
            for &g_idx in &items[c_idx].child_indices {
                assert_eq!(items[g_idx].state, CheckState::Checked);
            }
        }

        // 再次点击工具根节点，应全部取消勾选
        toggle_item(&mut items, 0);
        assert_eq!(items[0].state, CheckState::Unchecked);
        for &c_idx in &items[0].child_indices {
            assert_eq!(items[c_idx].state, CheckState::Unchecked);
        }
    }

    #[test]
    fn test_child_toggle_updates_tool_root() {
        let manifest = make_test_manifest();
        let mut items = build_tree_items(&manifest);

        // 全不选
        deselect_all(&mut items);
        assert_eq!(items[0].state, CheckState::Unchecked);

        // 勾选某个孙节点 (cm)
        let skill_mod_idx = items[0].child_indices[0];
        let cm_idx = items[skill_mod_idx].child_indices[0];
        toggle_item(&mut items, cm_idx);

        assert_eq!(items[cm_idx].state, CheckState::Checked);
        assert_eq!(items[skill_mod_idx].state, CheckState::PartiallyChecked);
        assert_eq!(items[0].state, CheckState::PartiallyChecked);
    }

    #[test]
    fn test_tree_items_have_local_paths() {
        let manifest = make_test_manifest();
        let items = build_tree_items(&manifest);

        // 工具根节点应有 local_path
        assert!(items[0].local_path.is_some());
        let root_path = items[0].local_path.as_ref().unwrap();
        assert!(root_path.to_string_lossy().contains(".gemini"));

        // 模块节点与细项应有 local_path
        let skill_mod_idx = items[0].child_indices[0];
        assert!(items[skill_mod_idx].local_path.is_some());

        let cm_idx = items[skill_mod_idx].child_indices[0];
        assert!(items[cm_idx].local_path.is_some());
        let cm_path = items[cm_idx].local_path.as_ref().unwrap();
        assert!(cm_path.to_string_lossy().contains("cm"));
    }
}
