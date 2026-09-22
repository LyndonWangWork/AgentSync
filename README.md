# AgentSync (AI CLI Configuration Transfer & Sync Tool)

[English](README_EN.md) | **简体中文**

面向终端 AI 编程工具（Google Antigravity、Anthropic Claude Code、OpenAI Codex CLI、OpenCode 等）的全局配置迁移、备份与局域网同步工具（基于 Rust 构建）。

支持在不同机器或多环境之间无缝迁移自定义技能（Skills）、扩展插件（Plugins）、MCP 服务定义、全局规则（GEMINI.md / CLAUDE.md / AGENTS.md 等）、偏好设置（Settings / Config）、扩展钩子（Hooks）以及可选的认证凭据和运行状态。

---

## 核心特性

- **多工具原生支持**：
  - 以 AI 工具作为第一级根节点统一聚合管理。
  - 原生支持 Google Antigravity、Anthropic Claude Code、OpenAI Codex CLI、OpenCode 等主流 AI 终端工具。
- **三级树形交互向导 (TUI)**：
  - 第一层（根节点）：AI 工具（可一键全选/取消该工具全部资产）。
  - 第二层（模块）：自定义技能、扩展插件、MCP、全局规则、设置、钩子、凭据、状态等。
  - 第三层（细项）：技能名、插件名、单个配置文件。
  - 支持父子节点三态联动：全选、全不选、半选。
- **双传输通道**：
  - **离线归档模式**：导出自包含 `manifest.json` 多工具资产清单的 `.zip` 压缩归档包。
  - **局域网直传模式**：内置多线程 HTTP 传输引擎，支持 6 位英数 PIN 码验证或 `--nopin` 免密传输，支持多客户端并发连接与独立下载。
- **安全保障机制**：
  - 默认保护账号凭据与历史状态数据，在树形列表中标注为 `[敏感]` 且默认不勾选，需用户手动勾选授权。
- **全套国际化 (i18n) 双语支持**：
  - 原生支持中文与英文无缝切换。
  - 自动识别：根据当前操作系统区域语言自动选择中英文界面。
  - 全局参数：支持 `--lang <zh|en|auto>` 手动覆盖，或通过环境变量 `AGENTSYNC_LANG` (兼容 `AITRANS_LANG`) 指定。
  - 实时切换：在 TUI 交互菜单中提供一键切换语言功能，即时生效。
- **智能合并与快照回滚**：
  - 导入前默认自动创建时间戳快照（统一存放在 `~/.agentsync/backups/`），支持一键回滚。
  - 对 `mcp_config.json` 等文件执行键级合并。
  - 遇到同名文件提供交互确认、强制覆盖（`--force`）或自动跳过（`--skip-existing`）三种策略。

---

## 快速使用

### 1. 交互向导模式 (TUI)
直接在终端中运行 `agentsync`，即可进入主功能菜单：
```bash
agentsync
```

#### 树形多选交互操作快捷键
在以 AI 工具为根节点的树形多选列表中，通过键盘与鼠标完成交互：

| 操作方式 | 功能说明 |
| :--- | :--- |
| `↑` / `k` | 光标向上移动一行 |
| `↓` / `j` | 光标向下移动一行 |
| `Space` (空格) / **鼠标左键单击** | 切换当前项勾选状态（级联影响所有子节点，并向上级联刷新父节点） |
| `o` / `O` / **Ctrl + 鼠标左键单击** | **打开对应本地目录或文件**（Windows 下通过资源管理器打开并高亮定位，macOS 唤起 Finder） |
| **鼠标移动悬停** | **鼠标指向某行时在文字下方显示下划线高亮** |
| **鼠标滚轮滑动** | 向上或向下平滑滚动列表光标 |
| `a` / `A` / `→` | 全选所有工具下的常规项（自动跳过敏感凭据） |
| `n` / `N` / `←` | 全部取消勾选 |
| `PageUp` | 向上滚动一页 |
| `PageDown` | 向下滚动一页 |
| `Home` | 跳转至列表顶部 |
| `End` | 跳转至列表底部 |
| `Enter` (回车) | 确认当前勾选并进入下一步 |
| `Esc` / `q` / `Ctrl+C` | 取消当前操作并返回 |

---

## 命令行完整参数手册 (CLI Reference)

### 全局通用参数

以下参数可置于任意子命令前后使用：

| 参数 | 类型 | 可选值 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `--lang <LANG>` | 字符串 | `zh`, `en`, `auto` | `auto` | 指定界面语言。默认 `auto` 自动检测系统区域语言；也可通过环境变量 `AGENTSYNC_LANG` (兼容 `AITRANS_LANG`) 覆盖 |

---

### 1. 扫描与查看 (`agentsync list`)

扫描本机已安装的 AI 工具与配置资产并打印清单与简介。

#### 参数列表

| 参数 | 短参数 | 类型 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `--tool <TOOLS>` | `-t` | 字符串 | 全部工具 | 指定扫描的 AI 工具（逗号分隔，如 `antigravity,claude,codex`） |

#### 查看示例

```bash
# 查看所有已安装 AI 工具的配置资产
agentsync list

# 仅查看指定工具的配置资产
agentsync list -t claude
```

---

### 2. 导出配置 (`agentsync export` / `agentsync exp`)

将本机的配置资产打包导出为 `.zip` 归档文件。默认启动终端树形向导供用户精确挑选需导出的 AI 工具与配置项；指定 `--all` 可跳过向导直接全量导出常规资产。

#### 参数列表

| 参数 | 短参数 | 类型 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `--output <PATH>` | `-o` | 路径 | `./agentsync-export-<时间戳>.zip` | 指定导出压缩包的保存路径与文件名 |
| `--tool <TOOLS>` | `-t` | 字符串 | 全部工具 | 指定包含的 AI 工具范围（逗号分隔，如 `claude,antigravity`） |
| `--all` | | 标志 | `false` | 全量导出所有常规配置模块（跳过交互式树形选择向导） |

#### 导出示例

```bash
# 1. 交互式选择导出：启动以 AI 工具为根节点的树形向导，自主勾选需要打包的工具与模块
agentsync export

# 2. 仅针对指定工具进行交互式选择导出
agentsync export -t claude,antigravity -o ./ai-tools.zip

# 3. 全量免交互导出指定工具的所有常规模块（适合自动化脚本）
agentsync export -t claude --all -o ./claude-backup.zip
```

---

### 3. 导入配置 (`agentsync import` / `agentsync imp`)

从 `.zip` 归档包中提取并恢复配置到各 AI 工具的本地配置目录。默认自动读取包内多工具清单并启动树形选择向导；指定 `--all` 可跳过向导直接导入全部资产。

#### 参数列表

| 参数 | 短参数 | 类型 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `<FILE>` | (位置参数) | 路径 | 必填 | 待导入的 `.zip` 归档文件路径 |
| `--all` | | 标志 | `false` | 全量导入归档包内的全部配置模块（跳过树形选择向导） |
| `--force` | `-f` | 标志 | `false` | 遇到同名文件冲突时强制以导入包为准覆盖 |
| `--skip-existing`| | 标志 | `false` | 遇到同名文件冲突时保留本机原有文件，自动跳过 |
| `--no-backup` | | 标志 | `false` | 导入执行前跳过创建系统还原快照 |
| `--yes` | `-y` | 标志 | `false` | 自动确认导入，完全跳过终端人工按键交互 |

#### 导入示例

```bash
# 1. 交互式选择导入：自动启动树形列表，用户自主勾选导入项
agentsync import ./my-ai-backup.zip

# 2. 增量导入：遇到已有同名技能或文件时自动跳过保持本地版本
agentsync import ./my-ai-backup.zip --skip-existing

# 3. 全量无人值守导入：导入包内所有内容，强制覆盖且跳过确认
agentsync import ./my-ai-backup.zip --all --force -y
```

---

### 4. 局域网直传 - 发送端 (`agentsync send` / `agentsync serve`)

在当前机器启动局域网传输 HTTP 服务，等待目标机器连接并拉取配置包。默认启动树形多选向导打包选定资产，并保持常驻服务状态允许多个客户端随时拉取。

#### 参数列表

| 参数 | 短参数 | 类型 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `--port <PORT>` | `-p` | 整数 | `38992` | 指定服务监听端口（若固定端口被占用，自动回退到可用随机端口） |
| `--ip <IP>` | | 字符串 | 自动识别 | 指定对外显示的本机 IP 地址（默认优先推荐物理网卡/Wi-Fi 地址） |
| `--tool <TOOLS>` | `-t` | 字符串 | 全部工具 | 指定发送的 AI 工具范围（逗号分隔） |
| `--all` | | 标志 | `false` | 全量发送所有常规配置模块（跳过交互式树形选择向导） |
| `--nopin` | | 标志 | `false` | 禁用连接 PIN 码验证，允许客户端免密直接拉取 |
| `--once` | | 标志 | `false` | 首次传输完成后立即退出服务端（单次传输模式） |

#### 发送端示例

```bash
# 1. 交互式挑选发送项并启动直传（生成 6 位英数 PIN 码，持续监听多客户端）
agentsync send

# 2. 免交互全量发送（跳过树形向导，直接打包所有常规资产）
agentsync send --all

# 3. 免密直传（局域网受信环境下免去输入 PIN 码）
agentsync send --nopin

# 4. 单次模式（首台机器连接下载完成后自动关闭服务退出）
agentsync send --once

# 5. 指定固定端口与对外推荐 IP
agentsync send --port 38992 --ip 192.168.1.100
```

---

### 5. 局域网直传 - 接收端 (`agentsync receive` / `agentsync recv`)

连接局域网中运行的发送端机器，拉取配置并写入当前机器。默认展示以 AI 工具为根节点的树形向导供用户挑选接收项；指定 `--all` 可跳过向导全量导入。

#### 参数列表

| 参数 | 短参数 | 类型 | 默认值 | 详细说明 |
| :--- | :--- | :--- | :--- | :--- |
| `<ADDR>` | (位置参数) | 字符串 | 必填 | 发送端地址（如 `192.168.1.50` 或指定端口 `192.168.1.50:38992`） |
| `--pin <PIN>` | `-p` | 字符串 | 可选 | 6 位连接验证码（支持英文字母与数字，大小写不敏感） |
| `--nopin` | | 标志 | `false` | 免密直接连接（需服务端启用 `--nopin`） |
| `--all` | | 标志 | `false` | 全量接收并导入包内包含的全部配置模块（跳过树形向导） |
| `--force` | `-f` | 标志 | `false` | 遇到同名文件冲突时强制覆盖 |
| `--no-backup` | | 标志 | `false` | 导入前跳过创建备份快照 |
| `--yes` | `-y` | 标志 | `false` | 自动确认导入，跳过交互提示 |

#### 接收端示例

```bash
# 1. 交互式接收（拉取后弹出树形交互列表供用户自主勾选）
agentsync receive 192.168.1.50 --pin A8K2M9

# 2. 服务端开启免密时接收
agentsync receive 192.168.1.50 --nopin

# 3. 命令行交互连接并在同名文件冲突时强制覆盖
agentsync receive 192.168.1.50 --pin A8K2M9 --force

# 4. 全自动静默拉取全部配置（适合脚本部署）
agentsync receive 192.168.1.50 --pin A8K2M9 --all --force -y
```

---

### 6. 快照备份与回滚 (`agentsync backup`)

管理由导入操作自动产生的历史快照，保障数据安全。

#### 子命令列表

```bash
# 1. 列出所有历史备份快照
agentsync backup list

# 2. 从指定快照完整回滚还原配置
agentsync backup restore snapshot-20260921-084444
```

---

## 编译与安装

确保系统已安装 Rust 工具链（推荐 Rust 1.70+）：

```bash
# 编译 Debug 版本
cargo build

# 编译 Release 高性能独立可执行文件
cargo build --release

# 编译产物路径
# Windows: target/release/agentsync.exe
# Linux/macOS: target/release/agentsync
```

---

## 开源协议

本项目采用 [CC BY-NC-SA 4.0](LICENSE) (Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International) 协议授权。

- **非商业性使用**：禁止将本项目及其修改版本用于商业营利目的。
- **署名与相同方式共享**：任何分发均须保留原作者版权声明，且衍生版本须使用相同协议开源。
