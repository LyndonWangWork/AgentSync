use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolId {
    Antigravity,
    Claude,
    Codex,
    OpenCode,
}

impl ToolId {
    pub const ALL: &'static [ToolId] = &[
        ToolId::Antigravity,
        ToolId::Claude,
        ToolId::Codex,
        ToolId::OpenCode,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ToolId::Antigravity => "antigravity",
            ToolId::Claude => "claude",
            ToolId::Codex => "codex",
            ToolId::OpenCode => "opencode",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ToolId::Antigravity => "🤖 Google Antigravity",
            ToolId::Claude => "🧠 Anthropic Claude Code",
            ToolId::Codex => "💻 OpenAI Codex CLI",
            ToolId::OpenCode => "🌐 OpenCode",
        }
    }

    pub fn default_dir_name(&self) -> &'static str {
        match self {
            ToolId::Antigravity => ".gemini",
            ToolId::Claude => ".claude",
            ToolId::Codex => ".codex",
            ToolId::OpenCode => ".opencode",
        }
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ToolId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "antigravity" | "gemini" | "agy" => Ok(ToolId::Antigravity),
            "claude" | "claude-code" | "claudecode" => Ok(ToolId::Claude),
            "codex" | "codex-cli" => Ok(ToolId::Codex),
            "opencode" => Ok(ToolId::OpenCode),
            _ => Err(crate::i18n::t!(
                format!("未知 AI 工具: {}", s),
                format!("Unknown AI tool: {}", s)
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleId {
    Skills,
    Plugins,
    Mcp,
    Rules,
    Settings,
    Hooks,
    Credentials,
    State,
}

impl ModuleId {
    pub const ALL: &'static [ModuleId] = &[
        ModuleId::Skills,
        ModuleId::Plugins,
        ModuleId::Mcp,
        ModuleId::Rules,
        ModuleId::Settings,
        ModuleId::Hooks,
        ModuleId::Credentials,
        ModuleId::State,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleId::Skills => "skills",
            ModuleId::Plugins => "plugins",
            ModuleId::Mcp => "mcp",
            ModuleId::Rules => "rules",
            ModuleId::Settings => "settings",
            ModuleId::Hooks => "hooks",
            ModuleId::Credentials => "credentials",
            ModuleId::State => "state",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ModuleId::Skills => crate::t!("🎨 自定义技能", "🎨 Custom Skills"),
            ModuleId::Plugins => crate::t!("🧩 扩展插件", "🧩 Plugins"),
            ModuleId::Mcp => crate::t!("🔌 MCP配置", "🔌 MCP Config"),
            ModuleId::Rules => crate::t!("📜 全局规则", "📜 Global Rules"),
            ModuleId::Settings => crate::t!("⚙️ 偏好设置", "⚙️ Settings"),
            ModuleId::Hooks => crate::t!("🪝 扩展钩子", "🪝 Hooks"),
            ModuleId::Credentials => crate::t!("🔑 认证凭据", "🔑 Credentials"),
            ModuleId::State => crate::t!("💾 运行状态与历史", "💾 State & History"),
        }
    }

    pub fn is_sensitive(&self) -> bool {
        matches!(self, ModuleId::Credentials | ModuleId::State)
    }

    pub fn default_selected(&self) -> bool {
        !self.is_sensitive()
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ModuleId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "skills" | "skill" => Ok(ModuleId::Skills),
            "plugins" | "plugin" => Ok(ModuleId::Plugins),
            "mcp" => Ok(ModuleId::Mcp),
            "rules" | "rule" => Ok(ModuleId::Rules),
            "settings" | "setting" => Ok(ModuleId::Settings),
            "hooks" | "hook" => Ok(ModuleId::Hooks),
            "credentials" | "cred" | "auth" => Ok(ModuleId::Credentials),
            "state" | "history" => Ok(ModuleId::State),
            _ => Err(crate::i18n::t!(
                format!("未知模块: {}", s),
                format!("Unknown module: {}", s)
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityInfo {
    pub contains_credentials: bool,
    pub contains_state: bool,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub id: ModuleId,
    pub name: String,
    pub count: usize,
    pub items: Vec<String>,
    #[serde(default)]
    pub descriptions: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub id: ToolId,
    pub name: String,
    pub base_dir: String,
    pub modules: Vec<ModuleSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub archive_path: String,
    pub target_relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub tool: ToolId,
    pub module: ModuleId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub generated_at: DateTime<Utc>,
    pub source_platform: PlatformInfo,
    pub security: SecurityInfo,
    pub tools: Vec<ToolManifest>,
    pub files: Vec<FileEntry>,
}
