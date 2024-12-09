use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize, Clone)]
pub struct ShortcutConfig {
    pub name: String,
    pub key: String,
    pub prefix: String,
    pub suffix: String,
    pub desc: String,
}

#[derive(Deserialize, Serialize)]
pub struct EditorConfig {
    pub shortcuts: Vec<ShortcutConfig>,
}

impl Default for EditorConfig {
    fn default() -> Self {
        let shortcuts = vec![
            ShortcutConfig {
                name: "type-bold".to_string(),
                key: "ctrl+b".to_string(),
                prefix: "**".to_string(),
                suffix: "**".to_string(),
                desc: "Bold text".to_string(),
            },
            ShortcutConfig {
                name: "type-italic".to_string(),
                key: "ctrl+i".to_string(),
                prefix: "_".to_string(),
                suffix: "_".to_string(),
                desc: "Italic text".to_string(),
            },
            ShortcutConfig {
                name: "code".to_string(),
                key: "ctrl+k".to_string(),
                prefix: "`".to_string(),
                suffix: "`".to_string(),
                desc: "Inline code".to_string(),
            },
            ShortcutConfig {
                name: "link".to_string(),
                key: "ctrl+l".to_string(),
                prefix: "[".to_string(),
                suffix: "](url)".to_string(),
                desc: "Create link".to_string(),
            },
            ShortcutConfig {
                name: "type-h1".to_string(),
                key: "ctrl+h".to_string(),
                prefix: "# ".to_string(),
                suffix: "".to_string(),
                desc: "Heading".to_string(),
            },
        ];
        
        EditorConfig { shortcuts }
    }
} 