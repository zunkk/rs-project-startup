use std::env;
use std::error::Error;

use vergen_gitcl::{Build, Cargo, Emitter, Gitcl, Rustc, Sysinfo};

fn main() -> Result<(), Box<dyn Error>> {
    let build = Build::all_build();
    let cargo = Cargo::all_cargo();
    let gitcl = Gitcl::all_git();
    let rustc = Rustc::all_rustc();
    let si = Sysinfo::all_sysinfo();

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&gitcl)?
        .add_instructions(&rustc)?
        .add_instructions(&si)?
        .emit()?;

    let v = env::var("APP_VERSION").unwrap_or("".into());
    println!("cargo:rustc-env=APP_VERSION={}", v);
    println!("cargo:rerun-if-env-changed=APP_VERSION");

    Ok(())
}
