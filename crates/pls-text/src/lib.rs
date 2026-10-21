//! Buffers de texte et édition de base.

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use pls_base::{ErrorCode, PlsError, Result};
use ropey::Rope;

/// Buffer de texte en mémoire, adossé à un fichier optionnel.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self::from_text("")
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            path: None,
            dirty: false,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)?;
        Ok(Self {
            rope: Rope::from_str(&text),
            path: Some(path.to_path_buf()),
            dirty: false,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .clone()
            .ok_or_else(|| PlsError::config("buffer has no path; use save_as"))?;
        self.save_as(path)
    }

    pub fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = path.into();
        std::fs::write(&path, self.text())?;
        self.path = Some(path);
        self.dirty = false;
        Ok(())
    }

    pub fn insert(&mut self, char_index: usize, text: &str) -> Result<()> {
        if char_index > self.rope.len_chars() {
            return Err(PlsError::app(
                ErrorCode::INVALID_PARAMS,
                format!(
                    "char index {char_index} is outside the buffer ({} chars)",
                    self.rope.len_chars()
                ),
            ));
        }

        self.rope.insert(char_index, text);
        self.dirty = true;
        Ok(())
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn line(&self, line_index: usize) -> Option<String> {
        if line_index >= self.rope.len_lines() {
            return None;
        }

        Some(self.rope.line(line_index).to_string())
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_text_at_a_char_index() {
        let mut buffer = TextBuffer::from_text("fn main() {}\n");
        buffer.insert(3, "hello ").expect("insert");
        assert_eq!(buffer.text(), "fn hello main() {}\n");
        assert!(buffer.is_dirty());
    }

    #[test]
    fn rejects_out_of_bounds_insert() {
        let mut buffer = TextBuffer::from_text("");
        assert!(buffer.insert(1, "x").is_err());
    }

    #[test]
    fn saves_and_reopens_a_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("main.dart");

        let mut buffer = TextBuffer::from_text("void main() {}\n");
        buffer.save_as(&path).expect("save");
        assert!(!buffer.is_dirty());

        let reopened = TextBuffer::open(&path).expect("open");
        assert_eq!(reopened.text(), "void main() {}\n");
        assert_eq!(reopened.path(), Some(path.as_path()));
    }
}
