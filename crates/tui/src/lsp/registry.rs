//! 语言检测 + 将语言映射到处理它的 LSP 服务器二进制文件的固定字典。
//!
//! 特意保持小而精：十几种语言，每种语言对应一个硬编码的可执行文件名，
//! 以及一个可选的参数列表。用户可以通过
//! `~/.deepseek/config.toml` 中的 `[lsp.servers]` 覆盖默认值
//!（由 [`super::LspConfig`] 处理，而非此文件）。

use std::path::Path;

/// 一种我们知道如何向 LSP 服务器询问的语言。通过文件扩展名由
/// [`detect_language`] 检测。`Other` 是在我们没有该文件的 LSP 时使用的哨兵——
/// LSP 管理器将其视为"跳过"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Go,
    Python,
    TypeScript,
    JavaScript,
    C,
    Cpp,
    Other,
}

impl Language {
    /// Stable lowercase string used as the key in `[lsp.servers]` overrides
    /// and in log lines.
    #[must_use]
    pub fn as_key(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Other => "other",
        }
    }

    /// LSP `languageId` value used in `textDocument/didOpen`. We follow the
    /// LSP-spec values: `rust`, `go`, `python`, `typescript`, `javascript`,
    /// `c`, `cpp`.
    #[must_use]
    pub fn language_id(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Other => "plaintext",
        }
    }
}

/// Detect the language of `path` from its extension. Falls back to
/// `Language::Other` when the extension is unknown (or the file has none),
/// which signals "skip" to the manager.
#[must_use]
pub fn detect_language(path: &Path) -> Language {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext.to_ascii_lowercase(),
        None => return Language::Other,
    };
    match ext.as_str() {
        "rs" => Language::Rust,
        "go" => Language::Go,
        "py" | "pyi" => Language::Python,
        "ts" | "tsx" => Language::TypeScript,
        "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
        "c" | "h" => Language::C,
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Language::Cpp,
        _ => Language::Other,
    }
}

/// Fixed default for "what executable + args do we run for `lang`?".
/// Returns `None` when no LSP server is wired for that language. The TUI
/// config layer can override this dictionary at runtime.
#[must_use]
pub fn server_for(lang: Language) -> Option<(&'static str, &'static [&'static str])> {
    match lang {
        Language::Rust => Some(("rust-analyzer", &[])),
        Language::Go => Some(("gopls", &["serve"])),
        Language::Python => Some(("pyright-langserver", &["--stdio"])),
        Language::TypeScript | Language::JavaScript => {
            Some(("typescript-language-server", &["--stdio"]))
        }
        Language::C | Language::Cpp => Some(("clangd", &[])),
        Language::Other => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_rust_extension() {
        assert_eq!(detect_language(&PathBuf::from("foo.rs")), Language::Rust);
        assert_eq!(detect_language(&PathBuf::from("FOO.RS")), Language::Rust);
    }

    #[test]
    fn detects_unknown_as_other() {
        assert_eq!(
            detect_language(&PathBuf::from("notes.txt")),
            Language::Other
        );
        assert_eq!(detect_language(&PathBuf::from("README")), Language::Other);
    }

    #[test]
    fn detects_typescript_variants() {
        assert_eq!(
            detect_language(&PathBuf::from("foo.ts")),
            Language::TypeScript
        );
        assert_eq!(
            detect_language(&PathBuf::from("foo.tsx")),
            Language::TypeScript
        );
        assert_eq!(
            detect_language(&PathBuf::from("foo.js")),
            Language::JavaScript
        );
    }

    #[test]
    fn server_for_rust_is_rust_analyzer() {
        let (cmd, args) = server_for(Language::Rust).expect("rust has a server");
        assert_eq!(cmd, "rust-analyzer");
        assert!(args.is_empty());
    }

    #[test]
    fn server_for_other_is_none() {
        assert!(server_for(Language::Other).is_none());
    }
}
