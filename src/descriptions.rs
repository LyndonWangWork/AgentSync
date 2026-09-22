use crate::model::ModuleId;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub fn get_module_description(module: ModuleId) -> &'static str {
    match module {
        ModuleId::Skills => crate::t!("自定义 Agent 专业技能库", "Custom Agent skill library"),
        ModuleId::Plugins => crate::t!("IDE 与 CLI 增强扩展插件", "IDE and CLI extension plugins"),
        ModuleId::Mcp => crate::t!("MCP 外部工具与服务连接定义", "MCP tool and service connection definitions"),
        ModuleId::Rules => crate::t!("全局系统级提示词与行为规范", "Global system prompts and behavioral rules"),
        ModuleId::Settings => crate::t!("系统运行参数与外观偏好配置", "System runtime parameters and preferences"),
        ModuleId::Hooks => crate::t!("生命周期与执行流扩展钩子脚本", "Lifecycle and execution flow extension hooks"),
        ModuleId::Credentials => crate::t!("认证凭据与授权令牌 [敏感]", "Authentication credentials and tokens [Sensitive]"),
        ModuleId::State => crate::t!("本地会话运行状态与执行历史 [敏感]", "Local session states and execution history [Sensitive]"),
    }
}

/// 仅针对各 AI 工具官方底座标准文件提供已知说明（非技能、非插件、非钩子）
pub fn get_known_description(module: ModuleId, item: &str) -> Option<&'static str> {
    match module {
        ModuleId::Skills => None,
        ModuleId::Plugins => None,
        ModuleId::Hooks => None,
        ModuleId::Settings => {
            if item.contains("config.json") || item.contains("config.toml") {
                Some(crate::t!(
                    "全局核心配置 (外观主题、模型默认值及路径)",
                    "Core global configuration (theme, default models, paths)"
                ))
            } else if item.contains("settings.json") {
                Some(crate::t!(
                    "CLI 终端选项、运行策略与会话配置",
                    "CLI terminal settings, policies, and session options"
                ))
            } else if item.contains("version.json") {
                Some(crate::t!(
                    "CLI 版本与运行环境信息",
                    "CLI version and runtime environment info"
                ))
            } else {
                None
            }
        }
        ModuleId::Mcp => Some(crate::t!(
            "MCP 外部服务器连接与环境变量定义",
            "MCP external server connections and environment config"
        )),
        ModuleId::Rules => {
            if item.eq_ignore_ascii_case("GEMINI.md") {
                Some(crate::t!(
                    "Google Antigravity 全局系统提示词与 Agent 规范",
                    "Google Antigravity global system prompt and agent rules"
                ))
            } else if item.eq_ignore_ascii_case("CLAUDE.md") {
                Some(crate::t!(
                    "Anthropic Claude Code 全局系统级指令与规范",
                    "Anthropic Claude Code global system prompt and instructions"
                ))
            } else if item.eq_ignore_ascii_case("AGENTS.md") {
                Some(crate::t!(
                    "多 Agent 协作网络架构与职责规范",
                    "Multi-agent network architecture and responsibility rules"
                ))
            } else if item.eq_ignore_ascii_case("OPENCODE.md") {
                Some(crate::t!(
                    "OpenCode 行为指引与开发规则规范",
                    "OpenCode behavior guidelines and development standards"
                ))
            } else {
                Some(crate::t!("系统行为规则说明文件", "System behavioral rule document"))
            }
        }
        ModuleId::Credentials => {
            if item.contains("google_accounts") {
                Some(crate::t!(
                    "Google 账号 OAuth 认证会话与访问令牌",
                    "Google account OAuth session and access tokens"
                ))
            } else if item.contains("oauth") {
                Some(crate::t!(
                    "OAuth 客户端 ID 与 Secret 凭据密钥",
                    "OAuth client ID and client secret keys"
                ))
            } else if item.contains("auth.json") || item.contains("credentials") {
                Some(crate::t!(
                    "AI 平台认证身份凭据与 API Key",
                    "AI platform authentication credentials and API keys"
                ))
            } else {
                Some(crate::t!("本地敏感授权凭据", "Local sensitive auth credentials"))
            }
        }
        ModuleId::State => {
            if item.contains("state.json") {
                Some(crate::t!(
                    "本地会话状态与断点恢复元数据",
                    "Local session states and breakpoint recovery metadata"
                ))
            } else if item.contains("history") {
                Some(crate::t!(
                    "交互历史与 CLI 命令调用执行日志",
                    "Conversation history and CLI command execution logs"
                ))
            } else if item.contains("sessions") || item.contains("memories") {
                Some(crate::t!(
                    "会话历史与上下文持久化记忆",
                    "Session history and persistent context memory"
                ))
            } else {
                Some(crate::t!("本地运行状态数据", "Local runtime state data"))
            }
        }
    }
}

/// 通用方法：从技能目录中提取描述说明（从 SKILL.md 或 README.md 动态读取）
pub fn extract_skill_description(skill_dir: &Path) -> Option<String> {
    let skill_md = skill_dir.join("SKILL.md");
    if skill_md.exists() {
        if let Some(desc) = extract_description_from_markdown(&skill_md) {
            return Some(desc);
        }
    }

    let readme_md = skill_dir.join("README.md");
    if readme_md.exists() {
        if let Some(desc) = extract_description_from_markdown(&readme_md) {
            return Some(desc);
        }
    }

    None
}

/// 从 Markdown 文件中提取描述（优先提取 YAML Frontmatter 中的 description，其次提取首段非标题文字）
fn extract_description_from_markdown(file_path: &Path) -> Option<String> {
    if let Ok(content) = fs::read_to_string(file_path) {
        let lines: Vec<&str> = content.lines().collect();

        // 检查并解析 YAML Frontmatter
        if lines.first().map(|l| l.trim() == "---").unwrap_or(false) {
            let mut in_desc_block = false;
            let mut desc_lines = Vec::new();

            for line in lines.iter().skip(1) {
                let trimmed = line.trim();
                if trimmed == "---" {
                    break;
                }

                if let Some(desc) = trimmed.strip_prefix("description:") {
                    let clean = desc.trim().trim_matches('"').trim_matches('\'').trim();
                    if clean == ">" || clean == "|" || clean.is_empty() {
                        in_desc_block = true;
                    } else {
                        return Some(clean.to_string());
                    }
                } else if in_desc_block {
                    if line.starts_with("  ") || line.starts_with('\t') {
                        let sub_clean = trimmed.trim_matches('"').trim_matches('\'').trim();
                        if !sub_clean.is_empty() {
                            desc_lines.push(sub_clean);
                        }
                    } else {
                        break;
                    }
                }
            }

            if !desc_lines.is_empty() {
                return Some(desc_lines.join(" "));
            }
        }

        // 回退提取首个非标题、非引用的说明段落
        for line in lines {
            let trimmed = line.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("---")
                && !trimmed.starts_with('>')
                && !trimmed.starts_with("```")
                && trimmed.len() > 6
            {
                return Some(trimmed.chars().take(80).collect());
            }
        }
    }

    None
}

/// 通用方法：从插件目录中提取描述说明（依次扫描各标准元数据 JSON，再回退 README.md）
pub fn extract_plugin_description(plugin_dir: &Path) -> Option<String> {
    // 候选配置文件清单
    let candidate_json_files = [
        "package.json",
        "plugin.json",
        "gemini-extension.json",
        "extension.json",
        "manifest.json",
    ];

    for json_name in candidate_json_files {
        let json_path = plugin_dir.join(json_name);
        if json_path.exists() {
            if let Ok(content) = fs::read_to_string(&json_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(desc) = val.get("description").and_then(|d| d.as_str()) {
                        let clean = desc.trim();
                        if !clean.is_empty() {
                            return Some(clean.chars().take(80).collect());
                        }
                    }
                }
            }
        }
    }

    // 回退到 README.md
    let readme = plugin_dir.join("README.md");
    if readme.exists() {
        if let Some(desc) = extract_description_from_markdown(&readme) {
            return Some(desc);
        }
    }

    None
}

/// 通用方法：从钩子脚本中提取头部注释描述说明
pub fn extract_hook_description(hook_path: &Path) -> Option<String> {
    if let Ok(content) = fs::read_to_string(hook_path) {
        for line in content.lines().take(15) {
            let trimmed = line.trim();
            if trimmed.starts_with("#!") {
                continue;
            }

            let comment_text = if let Some(rest) = trimmed.strip_prefix("//") {
                Some(rest.trim())
            } else if let Some(rest) = trimmed.strip_prefix('#') {
                Some(rest.trim())
            } else { trimmed.strip_prefix(';').map(|rest| rest.trim()) };

            if let Some(text) = comment_text {
                let clean = text.trim_matches('-').trim_matches('=').trim();
                if !clean.is_empty()
                    && !clean.starts_with("gsd-hook-version")
                    && !clean.starts_with("shellcheck")
                    && clean.len() > 4
                {
                    return Some(clean.chars().take(80).collect());
                }
            }
        }
    }
    None
}

pub fn resolve_item_description(
    module: ModuleId,
    item: &str,
    descriptions_map: Option<&HashMap<String, String>>,
) -> String {
    let custom_opt = descriptions_map.and_then(|m| m.get(item)).map(|s| s.as_str());
    resolve_item_description_single(module, item, custom_opt)
}

/// 统一解析条目说明：优先使用通用方法动态提取的源说明，其次回退到底座核心配置字典
pub fn resolve_item_description_single(
    module: ModuleId,
    item: &str,
    custom_desc: Option<&str>,
) -> String {
    if let Some(desc) = custom_desc {
        let trimmed = desc.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    if let Some(known) = get_known_description(module, item) {
        return known.to_string();
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skills_not_hardcoded_in_known_descriptions() {
        assert_eq!(get_known_description(ModuleId::Skills, "banner-design"), None);
        assert_eq!(get_known_description(ModuleId::Skills, "brand"), None);
        assert_eq!(get_known_description(ModuleId::Skills, "opendesign"), None);
        assert_eq!(get_known_description(ModuleId::Plugins, "opendesign"), None);
        assert_eq!(get_known_description(ModuleId::Hooks, "rtk-hook-gemini.sh"), None);
    }

    #[test]
    fn test_core_platform_files_retained() {
        assert!(get_known_description(ModuleId::Rules, "GEMINI.md").is_some());
        assert!(get_known_description(ModuleId::Settings, "config.json").is_some());
        assert!(get_known_description(ModuleId::Mcp, "mcp_config.json").is_some());
    }
}
