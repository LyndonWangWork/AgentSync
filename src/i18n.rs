use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Zh => "zh",
            Language::En => "en",
        }
    }
}

static CURRENT_LANG: AtomicU8 = AtomicU8::new(0); // 0: Zh, 1: En

#[cfg(target_os = "windows")]
extern "system" {
    fn GetUserDefaultLocaleName(lpLocaleName: *mut u16, cchLocaleName: i32) -> i32;
}

pub fn detect_system_language() -> Language {
    // 1. 优先读取自定义环境变量
    if let Ok(val) = std::env::var("AGENTSYNC_LANG").or_else(|_| std::env::var("AITRANS_LANG")) {
        let v = val.trim().to_ascii_lowercase();
        if v.starts_with("zh") {
            return Language::Zh;
        } else if v.starts_with("en") {
            return Language::En;
        }
    }

    // 2. 读取标准 LANG / LC_ALL 环境变量
    for key in &["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(key) {
            let v = val.trim().to_ascii_lowercase();
            if v.starts_with("zh") {
                return Language::Zh;
            } else if v.starts_with("en") {
                return Language::En;
            }
        }
    }

    // 3. Windows 原生 API 检测
    #[cfg(target_os = "windows")]
    {
        let mut buf = [0u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if len > 1 {
            if let Ok(loc) = String::from_utf16(&buf[..(len as usize - 1)]) {
                let lower = loc.to_ascii_lowercase();
                if lower.starts_with("zh") {
                    return Language::Zh;
                }
            }
        }
    }

    // 默认英文
    Language::En
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh_cn" => Language::Zh,
            "en" | "en-us" | "en_us" => Language::En,
            _ => detect_system_language(),
        }
    }
}

impl From<&String> for Language {
    fn from(s: &String) -> Self {
        Language::from(s.as_str())
    }
}

impl From<String> for Language {
    fn from(s: String) -> Self {
        Language::from(s.as_str())
    }
}

pub fn set_language<L: Into<Language>>(lang: L) {
    let val = match lang.into() {
        Language::Zh => 0,
        Language::En => 1,
    };
    CURRENT_LANG.store(val, Ordering::SeqCst);
}

pub fn current_language() -> Language {
    match CURRENT_LANG.load(Ordering::SeqCst) {
        0 => Language::Zh,
        _ => Language::En,
    }
}

pub fn is_zh() -> bool {
    current_language() == Language::Zh
}

#[macro_export]
macro_rules! t {
    ($zh:expr, $en:expr) => {
        if $crate::i18n::is_zh() {
            $zh
        } else {
            $en
        }
    };
}

pub use crate::t;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_switch() {
        set_language(Language::Zh);
        assert!(is_zh());
        assert_eq!(t!("中文", "English"), "中文");

        set_language(Language::En);
        assert!(!is_zh());
        assert_eq!(t!("中文", "English"), "English");

        // 还原回中文
        set_language(Language::Zh);
    }
}
