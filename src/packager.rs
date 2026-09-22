use crate::discovery::DiscoveryResult;
use crate::importer::is_file_matching_filter;
use crate::model::{FileEntry, Manifest, ModuleId, PlatformInfo, SecurityInfo};
use chrono::Utc;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn build_candidate_manifest(discovery: &DiscoveryResult) -> Manifest {
    let has_credentials = discovery.files.iter().any(|f| f.module == ModuleId::Credentials);
    let has_state = discovery.files.iter().any(|f| f.module == ModuleId::State);

    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into());

    Manifest {
        version: "2.0.0".into(),
        generated_at: Utc::now(),
        source_platform: PlatformInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hostname,
        },
        security: SecurityInfo {
            contains_credentials: has_credentials,
            contains_state: has_state,
            encrypted: false,
        },
        tools: discovery.tools.clone(),
        files: discovery.files.clone(),
    }
}

pub fn filter_discovery_files(
    discovery: &DiscoveryResult,
    filter: &crate::importer::ImportFilter,
) -> Vec<FileEntry> {
    discovery
        .files
        .iter()
        .filter(|f| is_file_matching_filter(f, filter))
        .cloned()
        .collect()
}

pub fn create_archive<W: Write + Seek>(
    writer: W,
    discovery: &DiscoveryResult,
    selected_files: &[FileEntry],
) -> io::Result<()> {
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let has_credentials = selected_files.iter().any(|f| f.module == ModuleId::Credentials);
    let has_state = selected_files.iter().any(|f| f.module == ModuleId::State);

    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into());

    let active_tools: Vec<_> = discovery
        .tools
        .iter()
        .filter_map(|t| {
            let tool_files: Vec<_> = selected_files.iter().filter(|f| f.tool == t.id).collect();
            if tool_files.is_empty() {
                None
            } else {
                let mut t_clone = t.clone();
                let active_modules: Vec<_> = t
                    .modules
                    .iter()
                    .filter_map(|m| {
                        let matching_files: Vec<_> = tool_files.iter().filter(|f| f.module == m.id).collect();
                        if matching_files.is_empty() {
                            None
                        } else {
                            let mut m_clone = m.clone();
                            if m.id == ModuleId::Skills || m.id == ModuleId::Plugins || m.id == ModuleId::Hooks {
                                m_clone.items.retain(|item| {
                                    matching_files.iter().any(|f| crate::importer::file_matches_item(f, item))
                                });
                            }
                            m_clone.count = matching_files.len();
                            Some(m_clone)
                        }
                    })
                    .collect();
                t_clone.modules = active_modules;
                Some(t_clone)
            }
        })
        .collect();

    let manifest = Manifest {
        version: "2.0.0".into(),
        generated_at: Utc::now(),
        source_platform: PlatformInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            hostname,
        },
        security: SecurityInfo {
            contains_credentials: has_credentials,
            contains_state: has_state,
            encrypted: false,
        },
        tools: active_tools,
        files: selected_files.to_vec(),
    };

    // 1. 写入 manifest.json
    zip.start_file("manifest.json", options)
        .map_err(io::Error::other)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(io::Error::other)?;
    zip.write_all(&manifest_bytes)?;

    // 2. 写入各个配置数据文件
    let mut buffer = [0u8; 16384];
    for file_entry in selected_files {
        if let Some(src_path) = discovery.file_source_paths.get(&file_entry.archive_path) {
            if src_path.exists() {
                zip.start_file(&file_entry.archive_path, options)
                    .map_err(io::Error::other)?;
                let mut f = File::open(src_path)?;
                loop {
                    let n = f.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    zip.write_all(&buffer[..n])?;
                }
            }
        }
    }

    zip.finish()
        .map_err(io::Error::other)?;
    Ok(())
}

pub fn export_to_file(
    output_path: &Path,
    discovery: &DiscoveryResult,
    selected_files: &[FileEntry],
) -> io::Result<()> {
    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(output_path)?;
    create_archive(file, discovery, selected_files)
}

pub fn export_to_memory(
    discovery: &DiscoveryResult,
    selected_files: &[FileEntry],
) -> io::Result<Vec<u8>> {
    let mut cursor = io::Cursor::new(Vec::new());
    create_archive(&mut cursor, discovery, selected_files)?;
    Ok(cursor.into_inner())
}
