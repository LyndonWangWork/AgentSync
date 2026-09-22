# aitrans 架构与数据格式设计规范

## 1. 系统概述

`aitrans` 是一款采用 Rust 编写的 Antigravity CLI 配置迁移与同步工具，提供跨机器的配置导出、导入、备份、合并以及局域网免盘互传能力。

### 核心特性
- **双模交互**：支持终端图形交互向导（TUI）与命令行子命令（CLI）。
- **模块化选择**：按模块自由勾选待迁移配置项（技能、插件、MCP、规则、设置、钩子）。
- **安全过滤**：自动隔离会话历史、机器唯一标识及身份认证凭据。
- **双传输通道**：支持离线压缩归档包（`.zip`）与局域网直连传输（PIN码认证）。
- **智能合并与回滚**：导入前自动创建快照备份；JSON配置采用键级合并；同名冲突支持交互判定。

---

## 2. 软件架构

```
+-------------------------------------------------------------+
|                      用户交互层                             |
|    CLI 子命令 (clap)            TUI 交互向导 (inquire)      |
+------------------------------+------------------------------+
                               |
+------------------------------v------------------------------+
|                         业务编排层                          |
|         扫描 (Scan)  |  打包 (Export)  |  恢复 (Import)     |
|         直传源 (Send)|  直传端 (Recv)  |  快照管理 (Backup) |
+------------------------------+------------------------------+
                               |
+------------------------------v------------------------------+
|                         核心引擎层                          |
|   +-------------------+  +-------------------+  +---------+ |
|   | 发现器 Discovery  |  | 归档引擎 Packager |  | 合并器  | |
|   +-------------------+  +-------------------+  | Merger  | |
|   | 安全过滤器 Filter |  | 网络传输 Transfer |  +---------+ |
|   +-------------------+  +-------------------+              |
+------------------------------+------------------------------+
                               |
+------------------------------v------------------------------+
|                         系统接入层                          |
|     本地文件系统 (std::fs)     |    平台路径解析 (dirs)     |
+-------------------------------------------------------------+
```

### 关键依赖选型
- **CLI & TUI**：`clap` (derive 模式) + `inquire` (交互式单选/多选/确认)
- **归档与压缩**：`zip` (deflate 压缩)
- **序列化与哈希**：`serde` + `serde_json` + `sha2`
- **网络传输**：`tiny_http` (源端轻量级单次服务) + `ureq` (目标端客户端拉取)
- **路径与系统**：`dirs` (支持 Windows、macOS、Linux 家目录标准规范) + `walkdir` + `chrono`

---

## 3. 配置项扫描与安全过滤矩阵

### 3.1 资产发现清单
| 模块标识 | 类别 | 源文件 / 目录路径 | 说明 |
| :--- | :--- | :--- | :--- |
| `skills` | 自定义技能 | `~/.gemini/config/skills/` | 全局技能目录，包含每个技能的 `SKILL.md` 及子资源 |
| `plugins` | 扩展插件 | `~/.gemini/config/plugins/` | 全局插件目录 |
| `mcp` | MCP 服务器 | `~/.gemini/config/mcp_config.json` | 外部工具及 MCP 客户端注册信息 |
| `rules` | 全局规则 | `~/.gemini/GEMINI.md` | 全局指令与系统行为规则 |
| `settings` | CLI 偏好设置 | `~/.gemini/config/config.json` 及 `~/.gemini/antigravity-cli/settings.json` | 基础环境配置参数 |
| `hooks` | 扩展钩子 | `~/.gemini/hooks/` | 自定义生命周期钩子脚本 |
| `credentials` *(可选)* | 认证凭据 | `~/.gemini/google_accounts.json`, `~/.gemini/oauth_creds.json` | Google账号与OAuth登录凭据（需用户显式勾选与确认） |
| `state` *(可选)* | 运行时状态与历史 | `~/.gemini/state.json`, `~/.gemini/antigravity-cli/history.jsonl` | 机器状态与CLI会话执行历史（需用户显式勾选） |

### 3.2 敏感数据选择与安全防护规则
敏感数据默认处于未勾选状态，由用户自主决定是否迁移：
- **用户显式授权**：在 TUI 勾选包含 `credentials` 或使用 CLI 参数 `--include-credentials` 时，终端展示明确的安全提示并要求用户输入 `yes` 二次确认。
- **归档加密保护**：迁移包含敏感凭据时，支持通过 `--password <密码>` 或交互提示为 `.zip` 归档包设置 AES-256 加密。
- **底层强制排除项**：临时与运行过程垃圾数据始终强制排除（`*.log`, `tmp/`, `crashes/`, `cache/`, `node_modules/`, `.git/`）。

---

## 4. 归档包结构与 Manifest 规范

导出生成的 `.zip` 文件具有自解释性，包含配置内容与元数据描述清单。

### 4.1 归档目录结构
```
aitrans-export-20260921-163000.zip
├── manifest.json              # 迁移清单与校验信息
├── data/
│   ├── skills/
│   │   ├── cm/
│   │   │   └── SKILL.md
│   │   └── faq/
│   │       └── SKILL.md
│   ├── plugins/
│   ├── mcp/
│   │   └── mcp_config.json
│   ├── rules/
│   │   └── GEMINI.md
│   ├── settings/
│   │   └── config.json
│   └── hooks/
```

### 4.2 `manifest.json` 数据模式
```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "version": "1.0.0",
  "generated_at": "2026-09-21T16:30:00Z",
  "source_platform": {
    "os": "windows",
    "arch": "x86_64",
    "hostname": "workstation-1"
  },
  "security": {
    "contains_credentials": false,
    "contains_state": false,
    "encrypted": false
  },
  "modules": [
    {
      "id": "skills",
      "name": "Custom Skills",
      "count": 2,
      "items": ["cm", "faq"]
    },
    {
      "id": "mcp",
      "name": "MCP Configuration",
      "count": 1,
      "items": ["mcp_config.json"]
    },
    {
      "id": "rules",
      "name": "Global Rules",
      "count": 1,
      "items": ["GEMINI.md"]
    }
  ],
  "files": [
    {
      "archive_path": "data/skills/cm/SKILL.md",
      "target_relative_path": ".gemini/config/skills/cm/SKILL.md",
      "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "size_bytes": 1024
    }
  ]
}
```

---

## 5. 导入流程、智能合并与备份规范

### 5.1 备份机制
每次执行导入前，工具自动在目标机器创建完整快照：
- **备份目录**：`~/.gemini/backups/snapshot-<timestamp>/`
- **元数据**：记录本次导入触发前的现有配置状态，支持 `aitrans rollback` 恢复至指定快照。

### 5.2 冲突解决与合并策略
- **JSON 智能合并（针对 `mcp_config.json` 与设置文件）**：
  ```
  目标机现有 mcpServers 键值 + 源机导入 mcpServers 键值 -> 合并字典
  - 遇到同名服务名：提示用户选择保留目标机版本或覆盖为源机版本（或通过 --force 默认覆盖）。
  ```
- **技能与插件目录冲突**：
  - 目标机已存在同名技能文件夹（如 `cm/`）：
    - 交互模式下提供选项：`[覆盖] / [跳过] / [重命名为 cm-imported]`；
    - CLI 模式下遵循 `--force`（覆盖）或 `--skip-existing`（跳过）。
- **文本规则文件（`GEMINI.md`）**：
  - 目标机已存在时提供选项：`[覆盖] / [追加合并] / [跳过]`。

---

## 6. 局域网直传协议 (LAN Direct Transfer)

### 6.1 传输工作流
```
 源主机 (Machine A)                               目标主机 (Machine B)
+-------------------+                             +--------------------+
| 执行 aitrans send |                             | 执行 aitrans recv  |
| 扫描并打包到内存  |                             | 指定主机及PIN码    |
| 启动 HTTP 监听    |                             |                    |
| 显示: IP, 端口, PIN|                             |                    |
+---------+---------+                             +---------+----------+
          |                                                 |
          |       1. GET /handshake?pin=XXXXXX              |
          |<------------------------------------------------+
          |       2. 响应 200 OK (验证通过)                |
          +------------------------------------------------>|
          |                                                 |
          |       3. GET /download?pin=XXXXXX               |
          |<------------------------------------------------+
          |       4. 数据流 (ZIP 流式传输)                  |
          +------------------------------------------------>|
          |                                                 |
          |                                           解包并执行安全导入
          v 传输完成自动关闭连接                            v 打印导入摘要
```

### 6.2 安全特性
- **PIN 码认证**：随机生成 6 位动态 PIN 码，仅验证成功后开放配置归档下载。
- **单次使用**：配置归档传输成功后立即自动终止网络监听。

---

## 7. CLI 命令与 TUI 规格设计

### 7.1 CLI 子命令接口
```bash
# 无参数运行启动 TUI 交互向导
aitrans

# 查看当前机器检测到的可迁移配置资产
aitrans list

# 导出选定配置到压缩包（普通资产）
aitrans export --output ./antigravity-backup.zip
aitrans export --skills cm,faq --mcp --rules -o ./custom.zip
aitrans export --all -o ./all-config.zip

# 导出包含敏感凭据并加密归档包
aitrans export --all --include-credentials --include-state --password "MySecretPass123" -o ./secure-backup.zip

# 导入配置归档（支持解密与强制覆盖）
aitrans import ./antigravity-backup.zip
aitrans import ./secure-backup.zip --password "MySecretPass123"
aitrans import ./antigravity-backup.zip --force --no-backup

# 局域网直传
aitrans send                          # 启动发送端（默认普通资产）
aitrans send --include-credentials    # 启动发送端（包含凭据，提示确认）
aitrans receive 192.168.1.100:8899    # 目标机拉取并执行导入

# 备份与快照管理
aitrans backup list
aitrans backup restore <snapshot-id>
```

### 7.2 TUI 交互界面流程
1. **主菜单**：
   - 📦 导出配置为压缩包 (Export)
   - 📥 从压缩包导入配置 (Import)
   - 📡 局域网发送配置 (Send)
   - 📥 局域网接收配置 (Receive)
   - 🔍 查看本地配置资产 (List)
   - ⏪ 管理历史备份快照 (Backups)
2. **多选配置项选择界面**：使用空格键勾选对应模块或细分项目：
   - [x] 🎨 自定义技能 (skills)
   - [x] 🧩 扩展插件 (plugins)
   - [x] 🔌 MCP 服务器配置 (mcp)
   - [x] 📜 全局规则 (GEMINI.md)
   - [x] ⚙️ 偏好设置 (settings)
   - [x] 🪝 自定义钩子 (hooks)
   - [ ] 🔑 敏感凭据 (google_accounts.json, oauth_creds.json) *(需输入 yes 确认)*
   - [ ] 💾 运行状态与会话历史 (state.json, history.jsonl)
3. **安全加密与确认阶段**：
   - 勾选 `credentials` 时触发安全警示弹窗，输入 `yes` 方可继续。
   - 提示是否为归档包设置解压访问密码（按 Enter 跳过或输入密码保护）。
