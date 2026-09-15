//! A `[[skill]]` entry with `gate = "none"` reads identically to real enforcement unless something says so out loud.
//!
//! gate names are checked against fixed constants, not the rails file's own text: a literal gate name in that file must not define itself.

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult, NAMES};
use crate::config::GATE_NAMES;

const CONFIG: &str = "harness.toml";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    if ctx.cfg.skill.is_empty() {
        return Ok(vec![common::finding(
            CONFIG,
            0,
            "declares no skills, so nothing records what the role prompts rely on or what fails without each one",
        )]);
    }
    let rails = common::rails_file(ctx.cfg);
    let mut known: String = NAMES.join("\n");
    known.push('\n');
    known.push_str(&GATE_NAMES.join("\n"));
    if common::exists(ctx.root, &rails) {
        known.push('\n');
        known.push_str(&common::read(ctx.root, &rails)?);
    }

    let mut found = Vec::new();
    for skill in &ctx.cfg.skill {
        let gate = if skill.gate.is_empty() {
            "none"
        } else {
            skill.gate.as_str()
        };
        if gate == "none" {
            found.push(common::finding(
                CONFIG,
                0,
                format!(
                    "{} is declared with gate: none -- nothing fails without it, so relying on it is a hope",
                    skill.id
                ),
            ));
            continue;
        }
        // word-boundary, not substring: a short gate name matches some word in every file otherwise
        let word = common::re(&format!(r"\b{}\b", regex::escape(gate)))?;
        if !word.is_match(&known) {
            found.push(common::finding(
                CONFIG,
                0,
                format!(
                    "{} names gate {gate}, which neither the built-in gate and probe names nor {rails} defines",
                    skill.id
                ),
            ));
        }
    }
    Ok(found)
}
