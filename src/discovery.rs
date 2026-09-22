use crate::model::{FileEntry, ModuleId, ModuleSummary, ToolId, ToolManifest};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct DiscoveryResult {
    pub tools: Vec<ToolManifest>,
    pub files: Vec<FileEntry>,
    pub file_source_paths: HashMap<String, PathBuf>, // archive_path -> actual filesystem path
}

pub fn calculate_sha256(path: &Path) -> io::Result<String> {
    let metadata = path.metadata()?;
    let size = metadata.len();
    if size > 10 * 1024 * 1024 {
        let mut hasher = Sha256::new();
        hasher.update(size.to_le_bytes());
        return Ok(hex::encode(hasher.finalize()));
    }

    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn should_skip_dir(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    #[allow(clippy::match_like_matches_macro)]
    match name.as_ref() {
        ".git"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | ".cache"
            | "cache"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".idea"
            | ".vscode" => true,
        _ => false,
    }
}

pub fn scan_configurations(
    selected_tools: Option<&[ToolId]>,
) -> io::Result<DiscoveryResult> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            crate::i18n::t!(
                "未能获取当前用户主目录",
                "Failed to get current user home directory"
            ),
        )
    })?;

    let tools_to_scan: Vec<ToolId> = match selected_tools {
        Some(tools) if !tools.is_empty() => tools.to_vec(),
        _ => ToolId::ALL.to_vec(),
    };

    let mut all_tools = Vec::new();
    let mut all_files = Vec::new();
    let mut all_source_paths = HashMap::new();

    for tool in tools_to_scan {
        if let Some(tool_manifest) = scan_single_tool(
            tool,
            &home_dir,
            &mut all_files,
            &mut all_source_paths,
        ) {
            all_tools.push(tool_manifest);
        }
    }

    Ok(DiscoveryResult {
        tools: all_tools,
        files: all_files,
        file_source_paths: all_source_paths,
    })
}

fn scan_single_tool(
    tool: ToolId,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
) -> Option<ToolManifest> {
    let tool_base_rel = tool.default_dir_name();
    let tool_dir = home_dir.join(tool_base_rel);
    if !tool_dir.exists() || !tool_dir.is_dir() {
        return None;
    }

    let mut module_items: HashMap<ModuleId, Vec<String>> = HashMap::new();
    let mut custom_descriptions: HashMap<(ModuleId, String), String> = HashMap::new();
    let initial_file_count = files.len();

    match tool {
        ToolId::Antigravity => {
            scan_antigravity(
                &tool_dir,
                home_dir,
                files,
                file_source_paths,
                &mut module_items,
                &mut custom_descriptions,
            );
        }
        ToolId::Claude => {
            scan_claude(
                &tool_dir,
                home_dir,
                files,
                file_source_paths,
                &mut module_items,
                &mut custom_descriptions,
            );
        }
        ToolId::Codex => {
            scan_codex(
                &tool_dir,
                home_dir,
                files,
                file_source_paths,
                &mut module_items,
                &mut custom_descriptions,
            );
        }
        ToolId::OpenCode => {
            scan_opencode(
                &tool_dir,
                home_dir,
                files,
                file_source_paths,
                &mut module_items,
                &mut custom_descriptions,
            );
        }
    }

    if files.len() == initial_file_count {
        return None;
    }

    let mut modules = Vec::new();
    for id in ModuleId::ALL {
        if let Some(items) = module_items.get(id) {
            let mut desc_map = HashMap::new();
            for item in items {
                let custom_opt = custom_descriptions
                    .get(&(*id, item.clone()))
                    .map(|s| s.as_str());
                let desc = crate::descriptions::resolve_item_description_single(
                    *id,
                    item,
                    custom_opt,
                );
                if !desc.is_empty() {
                    desc_map.insert(item.clone(), desc);
                }
            }

            modules.push(ModuleSummary {
                id: *id,
                name: id.display_name().to_string(),
                count: items.len(),
                items: items.clone(),
                descriptions: desc_map,
            });
        }
    }

    Some(ToolManifest {
        id: tool,
        name: tool.display_name().to_string(),
        base_dir: tool_base_rel.to_string(),
        modules,
    })
}

fn scan_antigravity(
    tool_dir: &Path,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    let tool = ToolId::Antigravity;

    // 1. Skills
    let skills_dir = tool_dir.join("config").join("skills");
    scan_skills_dir(&skills_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 2. Plugins
    let plugins_dir = tool_dir.join("config").join("plugins");
    scan_plugins_dir(&plugins_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 3. MCP Config
    let mcp_paths = [
        tool_dir.join("config").join("mcp_config.json"),
        tool_dir.join("mcp_config.json"),
    ];
    for mcp_file in mcp_paths {
        if mcp_file.exists() && mcp_file.is_file() {
            let item_name = "mcp_config.json".to_string();
            add_file(
                &mcp_file,
                home_dir,
                &format!("data/{}/mcp/mcp_config.json", tool.as_str()),
                tool,
                ModuleId::Mcp,
                files,
                file_source_paths,
            );
            module_items.entry(ModuleId::Mcp).or_default().push(item_name);
            break;
        }
    }

    // 4. Global Rules
    let mut rules = Vec::new();
    let rule_candidates = ["GEMINI.md", "AGENTS.md"];
    for name in rule_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/rules/{}", tool.as_str(), name),
                tool,
                ModuleId::Rules,
                files,
                file_source_paths,
            );
            rules.push(name.to_string());
        }
    }
    if !rules.is_empty() {
        module_items.insert(ModuleId::Rules, rules);
    }

    // 5. Settings
    let mut settings = Vec::new();
    let cfg = tool_dir.join("config").join("config.json");
    if cfg.exists() && cfg.is_file() {
        add_file(
            &cfg,
            home_dir,
            &format!("data/{}/settings/config.json", tool.as_str()),
            tool,
            ModuleId::Settings,
            files,
            file_source_paths,
        );
        settings.push("config/config.json".into());
    }
    let cli_cfg = tool_dir.join("antigravity-cli").join("settings.json");
    if cli_cfg.exists() && cli_cfg.is_file() {
        add_file(
            &cli_cfg,
            home_dir,
            &format!("data/{}/settings/antigravity-cli-settings.json", tool.as_str()),
            tool,
            ModuleId::Settings,
            files,
            file_source_paths,
        );
        settings.push("antigravity-cli/settings.json".into());
    }
    if !settings.is_empty() {
        module_items.insert(ModuleId::Settings, settings);
    }

    // 6. Hooks
    let hooks_candidates = [tool_dir.join("config").join("hooks"), tool_dir.join("hooks")];
    for hooks_dir in hooks_candidates {
        if hooks_dir.exists() && hooks_dir.is_dir() {
            scan_hooks_dir(&hooks_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);
            break;
        }
    }

    // 7. Credentials (Sensitive)
    let mut creds = Vec::new();
    let cred_candidates = [
        ("google_accounts.json", tool_dir.join("google_accounts.json")),
        ("config/google_accounts.json", tool_dir.join("config").join("google_accounts.json")),
        ("oauth_creds.json", tool_dir.join("oauth_creds.json")),
    ];
    for (name, p) in cred_candidates {
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/credentials/{}", tool.as_str(), p.file_name().unwrap_or_default().to_string_lossy()),
                tool,
                ModuleId::Credentials,
                files,
                file_source_paths,
            );
            creds.push(name.to_string());
        }
    }
    if !creds.is_empty() {
        module_items.insert(ModuleId::Credentials, creds);
    }

    // 8. State (Sensitive)
    let mut state = Vec::new();
    let state_file = tool_dir.join("state.json");
    if state_file.exists() && state_file.is_file() {
        add_file(
            &state_file,
            home_dir,
            &format!("data/{}/state/state.json", tool.as_str()),
            tool,
            ModuleId::State,
            files,
            file_source_paths,
        );
        state.push("state.json".into());
    }
    let hist_file = tool_dir.join("antigravity-cli").join("history.jsonl");
    if hist_file.exists() && hist_file.is_file() {
        add_file(
            &hist_file,
            home_dir,
            &format!("data/{}/state/history.jsonl", tool.as_str()),
            tool,
            ModuleId::State,
            files,
            file_source_paths,
        );
        state.push("antigravity-cli/history.jsonl".into());
    }
    if !state.is_empty() {
        module_items.insert(ModuleId::State, state);
    }
}

fn scan_claude(
    tool_dir: &Path,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    let tool = ToolId::Claude;

    // 1. Skills
    let skills_dir = tool_dir.join("skills");
    scan_skills_dir(&skills_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 2. Plugins
    let plugins_dir = tool_dir.join("plugins");
    scan_plugins_dir(&plugins_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 3. MCP Config
    let mcp_candidates = ["mcp.json", "mcp_config.json", "mcp-needs-auth-cache.json"];
    let mut mcp_items = Vec::new();
    for name in mcp_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/mcp/{}", tool.as_str(), name),
                tool,
                ModuleId::Mcp,
                files,
                file_source_paths,
            );
            mcp_items.push(name.to_string());
        }
    }
    if !mcp_items.is_empty() {
        module_items.insert(ModuleId::Mcp, mcp_items);
    }

    // 4. Rules
    let mut rules = Vec::new();
    let rule_candidates = ["CLAUDE.md", "AGENTS.md"];
    for name in rule_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/rules/{}", tool.as_str(), name),
                tool,
                ModuleId::Rules,
                files,
                file_source_paths,
            );
            rules.push(name.to_string());
        }
    }
    if !rules.is_empty() {
        module_items.insert(ModuleId::Rules, rules);
    }

    // 5. Settings
    let mut settings = Vec::new();
    let settings_candidates = ["settings.json", "config.json"];
    for name in settings_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/settings/{}", tool.as_str(), name),
                tool,
                ModuleId::Settings,
                files,
                file_source_paths,
            );
            settings.push(name.to_string());
        }
    }
    if !settings.is_empty() {
        module_items.insert(ModuleId::Settings, settings);
    }

    // 6. Hooks
    let hooks_dir = tool_dir.join("hooks");
    if hooks_dir.exists() && hooks_dir.is_dir() {
        scan_hooks_dir(&hooks_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);
    }

    // 7. Credentials (Sensitive)
    let mut creds = Vec::new();
    let cred_candidates = [".credentials.json", "credentials.json", "auth.json"];
    for name in cred_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/credentials/{}", tool.as_str(), name),
                tool,
                ModuleId::Credentials,
                files,
                file_source_paths,
            );
            creds.push(name.to_string());
        }
    }
    if !creds.is_empty() {
        module_items.insert(ModuleId::Credentials, creds);
    }

    // 8. State (Sensitive)
    let mut state = Vec::new();
    let history_file = tool_dir.join("history.jsonl");
    if history_file.exists() && history_file.is_file() {
        add_file(
            &history_file,
            home_dir,
            &format!("data/{}/state/history.jsonl", tool.as_str()),
            tool,
            ModuleId::State,
            files,
            file_source_paths,
        );
        state.push("history.jsonl".into());
    }
    if !state.is_empty() {
        module_items.insert(ModuleId::State, state);
    }
}

fn scan_codex(
    tool_dir: &Path,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    let tool = ToolId::Codex;

    // 1. Skills
    let skills_dir = tool_dir.join("skills");
    scan_skills_dir(&skills_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 2. Plugins
    let plugins_dir = tool_dir.join("plugins");
    scan_plugins_dir(&plugins_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 3. MCP Config
    let mcp_file = tool_dir.join("mcp.json");
    if mcp_file.exists() && mcp_file.is_file() {
        add_file(
            &mcp_file,
            home_dir,
            &format!("data/{}/mcp/mcp.json", tool.as_str()),
            tool,
            ModuleId::Mcp,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Mcp, vec!["mcp.json".into()]);
    }

    // 4. Rules
    let mut rules = Vec::new();
    let rule_candidates = ["AGENTS.md", "CODEX.md"];
    for name in rule_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/rules/{}", tool.as_str(), name),
                tool,
                ModuleId::Rules,
                files,
                file_source_paths,
            );
            rules.push(name.to_string());
        }
    }
    if !rules.is_empty() {
        module_items.insert(ModuleId::Rules, rules);
    }

    // 5. Settings
    let mut settings = Vec::new();
    let settings_candidates = ["config.toml", "version.json"];
    for name in settings_candidates {
        let p = tool_dir.join(name);
        if p.exists() && p.is_file() {
            add_file(
                &p,
                home_dir,
                &format!("data/{}/settings/{}", tool.as_str(), name),
                tool,
                ModuleId::Settings,
                files,
                file_source_paths,
            );
            settings.push(name.to_string());
        }
    }
    if !settings.is_empty() {
        module_items.insert(ModuleId::Settings, settings);
    }

    // 6. Hooks
    let hooks_dir = tool_dir.join("hooks");
    if hooks_dir.exists() && hooks_dir.is_dir() {
        scan_hooks_dir(&hooks_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);
    }

    // 7. Credentials (Sensitive)
    let auth_file = tool_dir.join("auth.json");
    if auth_file.exists() && auth_file.is_file() {
        add_file(
            &auth_file,
            home_dir,
            &format!("data/{}/credentials/auth.json", tool.as_str()),
            tool,
            ModuleId::Credentials,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Credentials, vec!["auth.json".into()]);
    }

    // 8. State (Sensitive)
    let history_file = tool_dir.join("history.jsonl");
    if history_file.exists() && history_file.is_file() {
        add_file(
            &history_file,
            home_dir,
            &format!("data/{}/state/history.jsonl", tool.as_str()),
            tool,
            ModuleId::State,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::State, vec!["history.jsonl".into()]);
    }
}

fn scan_opencode(
    tool_dir: &Path,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    let tool = ToolId::OpenCode;

    // 1. Skills
    let skills_dir = tool_dir.join("skills");
    scan_skills_dir(&skills_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 2. Plugins
    let plugins_dir = tool_dir.join("plugins");
    scan_plugins_dir(&plugins_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);

    // 3. MCP Config
    let mcp_file = tool_dir.join("mcp.json");
    if mcp_file.exists() && mcp_file.is_file() {
        add_file(
            &mcp_file,
            home_dir,
            &format!("data/{}/mcp/mcp.json", tool.as_str()),
            tool,
            ModuleId::Mcp,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Mcp, vec!["mcp.json".into()]);
    }

    // 4. Rules
    let rule_file = tool_dir.join("OPENCODE.md");
    if rule_file.exists() && rule_file.is_file() {
        add_file(
            &rule_file,
            home_dir,
            &format!("data/{}/rules/OPENCODE.md", tool.as_str()),
            tool,
            ModuleId::Rules,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Rules, vec!["OPENCODE.md".into()]);
    }

    // 5. Settings
    let cfg_file = tool_dir.join("config.json");
    if cfg_file.exists() && cfg_file.is_file() {
        add_file(
            &cfg_file,
            home_dir,
            &format!("data/{}/settings/config.json", tool.as_str()),
            tool,
            ModuleId::Settings,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Settings, vec!["config.json".into()]);
    }

    // 6. Hooks
    let hooks_dir = tool_dir.join("hooks");
    if hooks_dir.exists() && hooks_dir.is_dir() {
        scan_hooks_dir(&hooks_dir, tool, home_dir, files, file_source_paths, module_items, custom_descriptions);
    }

    // 7. Credentials (Sensitive)
    let auth_file = tool_dir.join("auth.json");
    if auth_file.exists() && auth_file.is_file() {
        add_file(
            &auth_file,
            home_dir,
            &format!("data/{}/credentials/auth.json", tool.as_str()),
            tool,
            ModuleId::Credentials,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::Credentials, vec!["auth.json".into()]);
    }

    // 8. State (Sensitive)
    let history_file = tool_dir.join("history.jsonl");
    if history_file.exists() && history_file.is_file() {
        add_file(
            &history_file,
            home_dir,
            &format!("data/{}/state/history.jsonl", tool.as_str()),
            tool,
            ModuleId::State,
            files,
            file_source_paths,
        );
        module_items.insert(ModuleId::State, vec!["history.jsonl".into()]);
    }
}

fn scan_skills_dir(
    skills_dir: &Path,
    tool: ToolId,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    if !skills_dir.exists() || !skills_dir.is_dir() {
        return;
    }
    let mut skill_names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(desc) = crate::descriptions::extract_skill_description(&path) {
                    custom_descriptions.insert((ModuleId::Skills, name.clone()), desc);
                }
                skill_names.push(name.clone());

                for walk_entry in WalkDir::new(&path).into_iter().filter_entry(|e| !should_skip_dir(e)).flatten() {
                    let file_path = walk_entry.path();
                    if file_path.is_file() {
                        let rel = file_path.strip_prefix(skills_dir).unwrap_or(file_path);
                        add_file(
                            file_path,
                            home_dir,
                            &format!("data/{}/skills/{}", tool.as_str(), rel.to_slash_lossy()),
                            tool,
                            ModuleId::Skills,
                            files,
                            file_source_paths,
                        );
                    }
                }
            }
        }
    }
    if !skill_names.is_empty() {
        module_items.insert(ModuleId::Skills, skill_names);
    }
}

fn scan_plugins_dir(
    plugins_dir: &Path,
    tool: ToolId,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    if !plugins_dir.exists() || !plugins_dir.is_dir() {
        return;
    }
    let mut plugin_names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(plugins_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if let Some(desc) = crate::descriptions::extract_plugin_description(&path) {
                    custom_descriptions.insert((ModuleId::Plugins, name.clone()), desc);
                }
                plugin_names.push(name.clone());

                for walk_entry in WalkDir::new(&path).into_iter().filter_entry(|e| !should_skip_dir(e)).flatten() {
                    let file_path = walk_entry.path();
                    if file_path.is_file() {
                        let rel = file_path.strip_prefix(plugins_dir).unwrap_or(file_path);
                        add_file(
                            file_path,
                            home_dir,
                            &format!("data/{}/plugins/{}", tool.as_str(), rel.to_slash_lossy()),
                            tool,
                            ModuleId::Plugins,
                            files,
                            file_source_paths,
                        );
                    }
                }
            }
        }
    }
    if !plugin_names.is_empty() {
        module_items.insert(ModuleId::Plugins, plugin_names);
    }
}

fn scan_hooks_dir(
    hooks_dir: &Path,
    tool: ToolId,
    home_dir: &Path,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
    module_items: &mut HashMap<ModuleId, Vec<String>>,
    custom_descriptions: &mut HashMap<(ModuleId, String), String>,
) {
    let mut hook_names = Vec::new();
    for walk_entry in WalkDir::new(hooks_dir).into_iter().filter_entry(|e| !should_skip_dir(e)).flatten() {
        let file_path = walk_entry.path();
        if file_path.is_file() {
            let rel = file_path.strip_prefix(hooks_dir).unwrap_or(file_path);
            let rel_str = rel.to_string_lossy().to_string();
            if let Some(desc) = crate::descriptions::extract_hook_description(file_path) {
                custom_descriptions.insert((ModuleId::Hooks, rel_str.clone()), desc);
            }
            hook_names.push(rel_str.clone());
            add_file(
                file_path,
                home_dir,
                &format!("data/{}/hooks/{}", tool.as_str(), rel.to_slash_lossy()),
                tool,
                ModuleId::Hooks,
                files,
                file_source_paths,
            );
        }
    }
    if !hook_names.is_empty() {
        module_items.insert(ModuleId::Hooks, hook_names);
    }
}

fn add_file(
    file_path: &Path,
    home_dir: &Path,
    archive_path: &str,
    tool: ToolId,
    module: ModuleId,
    files: &mut Vec<FileEntry>,
    file_source_paths: &mut HashMap<String, PathBuf>,
) {
    let size_bytes = file_path.metadata().map(|m| m.len()).unwrap_or(0);
    let sha256 = calculate_sha256(file_path).unwrap_or_default();
    let target_relative_path = match file_path.strip_prefix(home_dir) {
        Ok(rel) => rel.to_slash_lossy(),
        Err(_) => file_path.to_slash_lossy(),
    };

    files.push(FileEntry {
        archive_path: archive_path.to_string(),
        target_relative_path,
        sha256,
        size_bytes,
        tool,
        module,
    });
    file_source_paths.insert(archive_path.to_string(), file_path.to_path_buf());
}

pub trait PathExt {
    fn to_slash_lossy(&self) -> String;
}

impl PathExt for Path {
    fn to_slash_lossy(&self) -> String {
        self.to_string_lossy().replace('\\', "/")
    }
}
