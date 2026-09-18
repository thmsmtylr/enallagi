use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};

const CITED: &str = r"^(bun|bunx|cargo|go|pytest|npm|npx|pnpm|yarn|jest|vitest|git|turbo|node|ps|sed|grep|touch|rm|chmod)\b";

pub fn cites_something(text: &str) -> Res<bool> {
    let cited = common::re(CITED)?;
    let dotted = common::re(r"\.\w")?;
    Ok(common::backticked(text)
        .iter()
        .any(|token| token.contains('/') || dotted.is_match(token) || cited.is_match(token)))
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    let learnings = common::instance(ctx, "LEARNINGS.md");
    for (at, text) in common::learning_entries(ctx)? {
        if !cites_something(&text)? {
            found.push(common::finding(
                &learnings,
                at,
                format!(
                    "entry names no file, command or hook: {}",
                    common::cut(text.trim(), 90)
                ),
            ));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_runner_command_reads_as_cited() {
        let mut commands: Vec<String> = crate::runners::presets()
            .into_iter()
            .map(|r| r.test_command)
            .collect();
        commands.extend(["pnpm test", "go test"].map(String::from));
        for command in commands {
            let entry = format!("- [seed] a red run → run `{command}` first");
            assert!(cites_something(&entry).expect("pattern"), "{entry}");
        }
    }
}
