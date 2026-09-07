//! agent: adapter presets — one static TOML file per supported coding agent,
//! embedded into the binary and parsed on demand.

use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TurnCap {
    Flag,
    Time,
    Config,
    None,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct UsagePaths {
    pub cost: Option<String>,
    pub input_tokens: Option<String>,
    pub output_tokens: Option<String>,
    pub turns: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Preset {
    pub name: String,
    pub argv: Vec<String>,
    pub turn_cap: TurnCap,
    #[serde(default)]
    pub usage: UsagePaths,
    #[serde(default)]
    pub skills_dir: Option<String>,
    #[serde(default)]
    pub invocation: Option<String>,
    #[serde(default)]
    pub instruction_file: Option<String>,
    #[serde(default)]
    pub hooks_file: Option<String>,
    #[serde(default)]
    pub hook_events: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub model_flag: Option<String>,
}

pub type Presets = BTreeMap<String, Preset>;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("preset {0} is not known")]
    UnknownPreset(String),
    #[error("preset custom needs [agent] command")]
    CustomWithoutCommand,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("stopped: STOP file appeared during a rate-limit wait")]
    Stopped,
}

const PRESET_FILES: &[(&str, &str)] = &[
    ("claude", include_str!("../adapters/presets/claude.toml")),
    ("codex", include_str!("../adapters/presets/codex.toml")),
    ("gemini", include_str!("../adapters/presets/gemini.toml")),
    (
        "opencode",
        include_str!("../adapters/presets/opencode.toml"),
    ),
    ("copilot", include_str!("../adapters/presets/copilot.toml")),
    ("goose", include_str!("../adapters/presets/goose.toml")),
    ("aider", include_str!("../adapters/presets/aider.toml")),
    ("amp", include_str!("../adapters/presets/amp.toml")),
    ("cursor", include_str!("../adapters/presets/cursor.toml")),
    ("kimi", include_str!("../adapters/presets/kimi.toml")),
    ("qwen", include_str!("../adapters/presets/qwen.toml")),
    ("omp", include_str!("../adapters/presets/omp.toml")),
    ("pi", include_str!("../adapters/presets/pi.toml")),
];

/// Parses the embedded preset files. A malformed embedded file is a build
/// defect, not a runtime condition, so this panics (via `expect`) rather
/// than returning a `Result`.
pub fn presets() -> Presets {
    PRESET_FILES
        .iter()
        .map(|(name, text)| {
            let preset: Preset = toml::from_str(text).expect(name);
            (name.to_string(), preset)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thirteen_presets_load() {
        let presets = presets();
        assert_eq!(presets.len(), 13);
        assert_eq!(presets["aider"].turn_cap, TurnCap::None);
        assert_eq!(
            presets["claude"].usage.cost,
            Some("total_cost_usd".to_string())
        );
        for (name, preset) in &presets {
            assert!(
                preset.argv.iter().any(|a| a.contains("{prompt}")),
                "{name} argv missing {{prompt}}"
            );
        }
    }
}
