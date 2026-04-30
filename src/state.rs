use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ctx::{git, Ctx};

/// Encode a ref name so it can be used as a file-system path component.
pub fn enc(r#ref: &str) -> String {
    r#ref.replace('/', "__SLASH__")
}

pub fn state_dir(ctx: &Ctx, name: &str) -> Result<PathBuf> {
    let gd = git(ctx, &["rev-parse", "--git-dir"])?;
    let gd_path = if Path::new(&gd).is_absolute() {
        PathBuf::from(&gd)
    } else {
        ctx.repo_root.join(&gd)
    };
    Ok(gd_path.join(format!("stack-rebase-{}", enc(name))))
}

pub fn save_tip(state_dir: &Path, kind: &str, branch: &str, sha: &str) -> Result<()> {
    let dir = state_dir.join(kind);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(enc(branch)), format!("{sha}\n"))?;
    Ok(())
}

pub fn load_tip(state_dir: &Path, kind: &str, branch: &str) -> Option<String> {
    let f = state_dir.join(kind).join(enc(branch));
    fs::read_to_string(f).ok().map(|s| s.trim().to_string())
}
