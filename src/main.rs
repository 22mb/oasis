use std::io::{Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
Generate an API client from an OpenAPI 3.2 document.

Usage: oasis <SPEC> [-o <FILE>] [--lang <LANG>]

Arguments:
  <SPEC>             OpenAPI document (JSON or YAML), or `-` for stdin

Options:
  -o, --output FILE  Write to FILE instead of stdout
  -l, --lang LANG    Target language [default: typescript]
  -h, --help         Print help
  -V, --version      Print version
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut spec = None;
    let mut output = None;
    let mut lang = oasis::Lang::TypeScript;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("oasis {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-o" | "--output" => output = Some(args.next().ok_or("missing value for --output")?),
            "-l" | "--lang" => {
                let name = args.next().ok_or("missing value for --lang")?;
                lang = oasis::Lang::from_name(&name)
                    .ok_or(format!("unsupported language `{name}`"))?;
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option `{s}`\n\n{USAGE}").into());
            }
            _ if spec.is_some() => return Err(format!("unexpected argument `{arg}`").into()),
            _ => spec = Some(arg),
        }
    }
    let spec = spec.ok_or(format!("missing <SPEC>\n\n{USAGE}"))?;

    let code = if spec == "-" {
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source)?;
        oasis::generate(&source, lang)?
    } else {
        oasis::generate_file(std::path::Path::new(&spec), lang)?
    };
    match output {
        Some(path) => {
            std::fs::write(&path, code).map_err(|e| format!("cannot write {path}: {e}"))?
        }
        None => std::io::stdout().write_all(code.as_bytes())?,
    }
    Ok(())
}
