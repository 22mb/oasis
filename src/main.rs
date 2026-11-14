use std::io::{Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
Generate an API client from an OpenAPI 3.0, 3.1 or 3.2 document.

Usage: oasis-gen <SPEC> [-o <FILE>]

Arguments:
  <SPEC>             OpenAPI document (JSON or YAML), or `-` for stdin

Options:
  -o, --output FILE  Write to FILE instead of stdout
  -h, --help         Print help
  -V, --version      Print version
";

fn main() -> ExitCode {
    // Reading and converting a document both recurse once per nesting level.
    // Windows gives the main thread a 1 MiB stack, which a debug build uses up
    // before hitting the depth limits, so run on a thread with a larger stack.
    let worker = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(run)
        .expect("cannot start a thread");
    match worker.join() {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut spec = None;
    let mut output = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("oasis-gen {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "-o" | "--output" => output = Some(args.next().ok_or("missing value for --output")?),
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option `{s}`\n\n{USAGE}").into());
            }
            _ if spec.is_some() => return Err(format!("unexpected argument `{arg}`").into()),
            _ => spec = Some(arg),
        }
    }
    let spec = spec.ok_or(format!("missing <SPEC>\n\n{USAGE}"))?;

    let lang = oasis_gen::Lang::TypeScript;
    let code = if spec == "-" {
        let mut source = String::new();
        std::io::stdin()
            .read_to_string(&mut source)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
        oasis_gen::generate(&source, lang)?
    } else {
        oasis_gen::generate_file(std::path::Path::new(&spec), lang)?
    };
    match output {
        Some(path) => {
            std::fs::write(&path, code).map_err(|e| format!("cannot write {path}: {e}"))?
        }
        None => match std::io::stdout().write_all(code.as_bytes()) {
            // If a reader like `head` stops early, do not report an error.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            result => result.map_err(|e| format!("cannot write to standard output: {e}"))?,
        },
    }
    Ok(())
}
