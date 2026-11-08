//! a `[[skill]]` with `gate = "none"` reads like real enforcement unless something says otherwise

use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult, NAMES};
// fixed constants, never the rails file's text: a gate name written there must not define itself
use crate::config::GATE_NAMES;

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let config_path = crate::config::config_path(ctx.root);
    let config = config_path.strip_prefix(ctx.root).unwrap_or(&config_path);
    let config = config.to_string_lossy();
    if ctx.cfg.skill.is_empty() {
        return Ok(vec![common::finding(
            &*config,
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
                &*config,
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
                &*config,
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
