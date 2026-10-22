use std::{env, error::Error, fs, path::PathBuf, process::ExitCode};
use wit_component::ComponentEncoder;

fn package() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let input = args.next().map(PathBuf::from);
    let output = args.next().map(PathBuf::from);
    let (Some(input), Some(output), None) = (input, output, args.next()) else {
        return Err("usage: atri-component <core.wasm> <component.wasm>".into());
    };
    if input == output {
        return Err("core module and component output must be different paths".into());
    }
    let module = fs::read(&input)?;
    // Bindgen embeds the shared WIT world in the core module. No Preview 1 or
    // Preview 2 adapters are installed: all imports use the native P3 ABI.
    let component = ComponentEncoder::default()
        .module(&module)?
        .validate(true)
        .encode()?;
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, component)?;
    println!("{}", output.display());
    Ok(())
}

fn main() -> ExitCode {
    match package() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("atri-component: {error}");
            ExitCode::FAILURE
        }
    }
}
