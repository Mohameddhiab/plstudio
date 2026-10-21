//! Inspecteur visuel Flutter.
//!
//! L'inspecteur émet une localisation ; il n'ouvre jamais un fichier.
//! C'est `pls-engine` qui appelle `editor.open`.

#![forbid(unsafe_code)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetSelection {
    pub widget: String,
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
}

impl WidgetSelection {
    pub fn location(&self) -> String {
        format!("{}:{}", self.file.display(), self.line)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorEvent {
    SelectionChanged(WidgetSelection),
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_file_line_location() {
        let selection = WidgetSelection {
            widget: "Padding".to_owned(),
            file: PathBuf::from("lib/main.dart"),
            line: 42,
            column: 3,
        };
        assert_eq!(selection.location(), "lib/main.dart:42");
    }
}
