//! A rail naming `test-hashes.json` as enforcement is only true if every file it names has a key there — rail-unenforced goes quiet once the file merely exists.

use super::common::{self, Res};
use super::rail_unenforced::is_path;
use super::{Finding, ProbeCtx, ProbeResult};

const HASHES: &str = "test-hashes.json";

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let rails = common::rails_file(ctx.cfg);
    let hashes = common::instance(ctx, HASHES);
    let keys: Option<Vec<String>> = if common::exists(ctx.root, &hashes) {
        let text = common::read(ctx.root, &hashes)?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{hashes}: {e}"))?;
        Some(
            value
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default(),
        )
    } else {
        None
    };

    let mut found = Vec::new();
    for row in common::rail_rows(ctx)? {
        if !row.cells[2].contains(HASHES) {
            continue;
        }
        let rail = row.cells[0].trim().to_string();
        for token in common::backticked(&row.cells[1]) {
            let token = token.trim().trim_end_matches('/').to_string();
            if token == HASHES || (!token.contains('/') && !is_path(&token)?) {
                continue;
            }
            if !common::exists(ctx.root, &token) {
                continue;
            }
            match &keys {
                None => found.push(common::finding(
                    &rails,
                    row.line,
                    format!(
                        "{rail} names {token} and {hashes} does not exist, so the rail covers nothing"
                    ),
                )),
                Some(keys) => {
                    let covered = keys
                        .iter()
                        .any(|k| *k == token || k.starts_with(&format!("{token}#")));
                    if !covered {
                        found.push(common::finding(
                            &rails,
                            row.line,
                            format!("{rail} names {token} and {hashes} has no key for it"),
                        ));
                    }
                }
            }
        }
    }
    Ok(found)
}
