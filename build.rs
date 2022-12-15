use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fs::{read, read_to_string, write};
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
    let welcome_info = read_to_string("resources/welcome.txt")?;
    let cargo = read("Cargo.toml")?;
    let config: CargoConfig = toml::from_slice(&cargo)?;

    let env = env::var_os("OUT_DIR").unwrap();
    let mut path = PathBuf::from(env);
    path.push("welcome_info");

    write(
        path,
        welcome_info.replace("${{version}}", config.package.version),
    )?;

    Ok(())
}
