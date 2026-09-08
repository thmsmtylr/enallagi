use super::common::{self, Res};
use super::{Finding, ProbeCtx, ProbeResult};
use crate::git;

fn allowed(ctx: &ProbeCtx, path: &str) -> bool {
    let layout = &ctx.cfg.layout;
    layout
        .allowed_prefixes
        .iter()
        .any(|prefix| path.starts_with(prefix.as_str()))
        || layout.docs.iter().any(|d| d == path)
        || layout.harness_allow.iter().any(|d| d == path)
}

fn machinery(ctx: &ProbeCtx, path: &str) -> bool {
    path.trim_matches('/').split('/').any(|part| {
        ctx.cfg.layout.machinery.iter().any(|m| m == part)
            || part.starts_with(".env")
            || part.ends_with(".tsbuildinfo")
    })
}

pub fn probe(ctx: &ProbeCtx) -> ProbeResult {
    common::result(find(ctx))
}

fn find(ctx: &ProbeCtx) -> Res<Vec<Finding>> {
    let mut found = Vec::new();
    for path in common::tracked(ctx.root)? {
        if !allowed(ctx, &path) {
            found.push(common::finding(
                &path,
                0,
                "tracked and neither product nor a document that governs it",
            ));
        }
    }

    let status = git::git(
        ctx.root,
        &["status", "--porcelain", "-uall", "--ignored", "-z"],
    )
    .map_err(|e| e.to_string())?;
    let records: Vec<&str> = status.split('\0').filter(|r| !r.is_empty()).collect();
    let mut skip = false;
    for record in records {
        if skip {
            skip = false;
            continue;
        }
        let (code, path) = match record.char_indices().nth(3).map(|(at, _)| at) {
            Some(at) => (&record[..2], &record[at..]),
            None => continue,
        };
        // a rename or a copy spends two NUL-separated fields on one change
        if code.starts_with('R') || code.starts_with('C') {
            skip = true;
        }
        if code != "??" && code != "!!" {
            continue;
        }
        if !allowed(ctx, path) && !machinery(ctx, path) {
            found.push(common::finding(
                path,
                0,
                "untracked or ignored, on no allowlist and no known machinery",
            ));
        }
    }
    found.sort_by(|a, b| (&a.path, a.line, &a.message).cmp(&(&b.path, b.line, &b.message)));
    Ok(found)
}
