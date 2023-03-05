use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs::write;
use std::path::PathBuf;

fn main() {
    _main().unwrap();
}

#[derive(Serialize, Deserialize)]
struct CargoConfig<'a> {
    #[serde(borrow = "'a")]
    package: Package<'a>,
}

#[derive(Serialize, Deserialize)]
struct Package<'a> {
    version: &'a str,
}

fn _main() -> Result<(), Box<dyn Error>> {
    let cargo = include_str!("Cargo.toml");
    let config: CargoConfig = toml::from_str(&cargo)?;

    let env = env::var_os("OUT_DIR").unwrap();
    let mut path = PathBuf::from(env);
    path.push("welcome_info");

    write(path, format_info(&config))?;

    Ok(())
}

fn format_info(cargo: &CargoConfig) -> String {
    format!(
        include_str!("resources/welcome.txt"),
        version = cargo.package.version
    )
}
