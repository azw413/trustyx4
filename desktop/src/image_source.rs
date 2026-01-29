use std::fs;
use std::path::{Path, PathBuf};

use trusty_core::image_viewer::{EntryKind, ImageData, ImageEntry, ImageError, ImageSource};
use trusty_epub::{BookCache, CacheStatus, CacheTocEntry};

pub struct DesktopImageSource {
    root: PathBuf,
}

impl DesktopImageSource {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn is_supported(name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        name.ends_with(".png")
            || name.ends_with(".jpg")
            || name.ends_with(".jpeg")
            || name.ends_with(".trimg")
            || name.ends_with(".tri")
            || name.ends_with(".epub")
            || name.ends_with(".epb")
    }

    fn resume_path(&self) -> PathBuf {
        self.root.join(".trusty_resume")
    }
}

impl ImageSource for DesktopImageSource {
    fn refresh(&mut self, path: &[String]) -> Result<Vec<ImageEntry>, ImageError> {
        let mut entries = Vec::new();
        let dir_path = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let read_dir = match fs::read_dir(&dir_path) {
            Ok(read_dir) => read_dir,
            Err(_) => return Ok(entries),
        };
        for entry in read_dir {
            let entry = entry.map_err(|_| ImageError::Io)?;
            let file_type = entry.file_type().map_err(|_| ImageError::Io)?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".trusty_resume" {
                continue;
            }
            if file_type.is_dir() {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::Dir,
                });
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if Self::is_supported(&name) {
                entries.push(ImageEntry {
                    name,
                    kind: EntryKind::File,
                });
            }
        }
        entries.sort_by(|a, b| {
            match (a.kind, b.kind) {
                (EntryKind::Dir, EntryKind::File) => std::cmp::Ordering::Less,
                (EntryKind::File, EntryKind::Dir) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        Ok(entries)
    }

    fn load(&mut self, path: &[String], entry: &ImageEntry) -> Result<ImageData, ImageError> {
        if entry.kind != EntryKind::File {
            return Err(ImageError::Unsupported);
        }
        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let path = base.join(&entry.name);
        let lower = entry.name.to_ascii_lowercase();
        if lower.ends_with(".epub") || lower.ends_with(".epb") {
            return Err(ImageError::Message("EPUB not implemented.".into()));
        }
        if lower.ends_with(".trimg") || lower.ends_with(".tri") {
            let data = fs::read(&path).map_err(|_| ImageError::Io)?;
            return parse_trimg(&data);
        }

        let data = fs::read(&path).map_err(|_| ImageError::Io)?;
        let image = image::load_from_memory(&data).map_err(|_| ImageError::Decode)?;
        let luma = image.to_luma8();
        Ok(ImageData::Gray8 {
            width: luma.width(),
            height: luma.height(),
            pixels: luma.into_raw(),
        })
    }

    fn save_resume(&mut self, name: Option<&str>) {
        let path = self.resume_path();
        if let Some(name) = name {
            let _ = fs::write(path, name.as_bytes());
        } else {
            let _ = fs::remove_file(path);
        }
    }

    fn epub_info(&mut self, path: &[String], entry: &ImageEntry) -> Option<String> {
        if entry.kind != EntryKind::File {
            return None;
        }
        let lower = entry.name.to_ascii_lowercase();
        if !lower.ends_with(".epub") && !lower.ends_with(".epb") {
            return None;
        }

        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let path = base.join(&entry.name);
        let cache_dir = trusty_epub::default_cache_dir(&path);
        match trusty_epub::load_or_build_cache(&path, &cache_dir) {
            Ok((cache, status)) => Some(format_epub_info(&cache, &status)),
            Err(err) => Some(format!("Failed to open EPUB:\n{err}")),
        }
    }

    fn epub_preview_text(&mut self, path: &[String], entry: &ImageEntry) -> Option<String> {
        if entry.kind != EntryKind::File {
            return None;
        }
        let lower = entry.name.to_ascii_lowercase();
        if !lower.ends_with(".epub") && !lower.ends_with(".epb") {
            return None;
        }

        let base = path.iter().fold(self.root.clone(), |acc, part| acc.join(part));
        let path = base.join(&entry.name);
        let cache_dir = trusty_epub::default_cache_dir(&path);
        let spine_count = trusty_epub::load_or_build_cache(&path, &cache_dir)
            .map(|(cache, _)| cache.spine.len())
            .unwrap_or(1);
        let max_try = spine_count.min(20).max(1);

        let mut last_snippet = String::new();
        let mut last_bytes = 0usize;
        let mut combined = String::new();
        for index in 0..max_try {
            let xhtml = match trusty_epub::read_spine_xhtml(&path, index) {
                Ok(xhtml) => xhtml,
                Err(_) => continue,
            };
            last_bytes = xhtml.len();
            last_snippet = xhtml.chars().take(400).collect::<String>();
            let blocks = match trusty_epub::parse_xhtml_blocks(&xhtml) {
                Ok(blocks) => blocks,
                Err(_) => continue,
            };
            let text = trusty_epub::blocks_to_plain_text(&blocks);
            let filtered = filter_preview_text(&text);
            if !filtered.trim().is_empty() {
                if !combined.is_empty() {
                    combined.push_str("\n\n");
                }
                combined.push_str(filtered.trim());
                if combined.len() > 2000 {
                    break;
                }
            }
        }

        if !combined.trim().is_empty() {
            return Some(combined);
        }

        Some(format!(
            "No preview text extracted.\n\nTried {} spine item(s).\nLast bytes: {}\nTop of file:\n{}",
            max_try, last_bytes, last_snippet
        ))
    }

    fn load_resume(&mut self) -> Option<String> {
        let path = self.resume_path();
        let data = fs::read(path).ok()?;
        let name = String::from_utf8_lossy(&data).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }
}

fn format_epub_info(cache: &BookCache, status: &CacheStatus) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Title: {}",
        cache.metadata.title.as_deref().unwrap_or("<unknown>")
    ));
    lines.push(format!(
        "Author: {}",
        cache.metadata.creator.as_deref().unwrap_or("<unknown>")
    ));
    lines.push(format!(
        "Language: {}",
        cache.metadata.language.as_deref().unwrap_or("<unknown>")
    ));
    lines.push(format!(
        "Identifier: {}",
        cache.metadata.identifier.as_deref().unwrap_or("<unknown>")
    ));
    lines.push(format!("OPF: {}", cache.opf_path));
    lines.push(format!(
        "Cover: {}",
        cache.cover_href.as_deref().unwrap_or("<none>")
    ));
    lines.push(format!("Spine items: {}", cache.spine.len()));
    lines.push(format!("TOC entries: {}", cache.toc.len()));
    lines.push(format!(
        "Cache: {}",
        if status.hit { "hit" } else { "miss" }
    ));
    lines.push(format!("Cache path: {}", status.cache_path.display()));
    lines.push(format!("Cache size: {} bytes", cache.source_size));

    if !cache.toc.is_empty() {
        lines.push("TOC preview:".to_string());
        let mut preview = Vec::new();
        collect_toc_preview_cache(&cache.toc, &mut preview, 8);
        for line in preview {
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn collect_toc_preview_cache(entries: &[CacheTocEntry], out: &mut Vec<String>, limit: usize) {
    for entry in entries.iter().take(limit) {
        let indent = "  ".repeat(entry.level as usize);
        let label = if entry.title.is_empty() {
            "<untitled>"
        } else {
            entry.title.as_str()
        };
        out.push(format!("{indent}- {label}"));
    }
}

fn filter_preview_text(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if trimmed.starts_with("[Image:") {
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out
}

fn parse_trimg(data: &[u8]) -> Result<ImageData, ImageError> {
    if data.len() < 16 || &data[0..4] != b"TRIM" {
        return Err(ImageError::Decode);
    }
    if data[4] != 1 || data[5] != 1 {
        return Err(ImageError::Unsupported);
    }
    let width = u16::from_le_bytes([data[6], data[7]]) as u32;
    let height = u16::from_le_bytes([data[8], data[9]]) as u32;
    let payload = &data[16..];
    let expected = ((width as usize * height as usize) + 7) / 8;
    if payload.len() != expected {
        return Err(ImageError::Decode);
    }
    Ok(ImageData::Mono1 {
        width,
        height,
        bits: payload.to_vec(),
    })
}
