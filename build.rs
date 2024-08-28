use vergen_git2::{Emitter, Git2Builder, RustcBuilder};

fn main() -> anyhow::Result<()> {
    let git2 = Git2Builder::default().sha(false).build()?;
    let rustc = RustcBuilder::default().semver(true).build()?;

    Emitter::default()
        .add_instructions(&git2)?
        .add_instructions(&rustc)?
        .emit()?;

    Ok(())
}
