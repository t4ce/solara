use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

use rust_qjs_dom::{DomArtifact, DomEngine};
use serde_json::{Value, json};

const HELP: &str = r#"RustQJSDom — renderer-free TrueSurfer DOM extraction

Usage:
  rust-qjs-dom [OPTIONS] [INPUT]
  rust-qjs-dom --jsonl

Arguments:
  INPUT                 HTML file to parse; omit it or use '-' for stdin

Options:
  --url URL             Source URL stored in the artifact
  -o, --output PATH     Write output to a file instead of stdout
  --compact             Emit compact JSON instead of pretty JSON
  --jsonl               Read {"url":"...","html":"..."} requests, one per line
  -h, --help            Show this help
  -V, --version         Show the version

Examples:
  rust-qjs-dom page.html --url https://example.test/page
  printf '<main>Hello</main>' | rust-qjs-dom --compact
  printf '%s\n' '{"url":"https://example.test/","html":"<h1>Hi</h1>"}' | rust-qjs-dom --jsonl
"#;

#[derive(Debug)]
struct Arguments {
    input: Option<PathBuf>,
    url: Option<String>,
    output: Option<PathBuf>,
    compact: bool,
    jsonl: bool,
}

fn parse_arguments() -> Result<Option<Arguments>, String> {
    let mut parsed = Arguments {
        input: None,
        url: None,
        output: None,
        compact: false,
        jsonl: false,
    };
    let mut arguments = env::args().skip(1);

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("rust-qjs-dom {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--url" => {
                parsed.url = Some(arguments.next().ok_or("--url requires a value")?);
            }
            "-o" | "--output" => {
                parsed.output = Some(PathBuf::from(
                    arguments.next().ok_or("--output requires a path")?,
                ));
            }
            "--compact" => parsed.compact = true,
            "--jsonl" => parsed.jsonl = true,
            "-" => {
                if parsed.input.replace(PathBuf::from("-")).is_some() {
                    return Err(String::from("only one input may be provided"));
                }
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}"));
            }
            value => {
                if parsed.input.replace(PathBuf::from(value)).is_some() {
                    return Err(String::from("only one input may be provided"));
                }
            }
        }
    }

    if parsed.jsonl
        && (parsed.input.is_some()
            || parsed.url.is_some()
            || parsed.output.is_some()
            || parsed.compact)
    {
        return Err(String::from(
            "--jsonl reads stdin and writes compact JSON to stdout; it cannot be combined with input/output options",
        ));
    }
    Ok(Some(parsed))
}

fn file_url(path: &Path) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    format!("file://{}", absolute.to_string_lossy())
}

fn read_html(input: Option<&Path>) -> Result<(String, String), Box<dyn Error>> {
    match input {
        Some(path) if path != Path::new("-") => {
            let html = fs::read_to_string(path)?;
            Ok((html, file_url(path)))
        }
        _ => {
            let mut html = String::new();
            io::stdin().read_to_string(&mut html)?;
            Ok((html, String::from("about:blank")))
        }
    }
}

fn write_artifact(
    artifact: &DomArtifact,
    compact: bool,
    output: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let mut bytes = if compact {
        serde_json::to_vec(artifact)?
    } else {
        serde_json::to_vec_pretty(artifact)?
    };
    bytes.push(b'\n');

    if let Some(path) = output {
        fs::write(path, bytes)?;
    } else {
        io::stdout().write_all(&bytes)?;
    }
    Ok(())
}

fn run_single(arguments: Arguments) -> Result<(), Box<dyn Error>> {
    let (html, inferred_url) = read_html(arguments.input.as_deref())?;
    let url = arguments.url.as_deref().unwrap_or(&inferred_url);
    let mut engine = DomEngine::new()?;
    let artifact = engine.parse(&html, url)?;
    write_artifact(&artifact, arguments.compact, arguments.output.as_deref())
}

fn jsonl_error(line: usize, message: impl ToString) -> Value {
    json!({
        "schema": "rustqjsdom.error",
        "schemaVersion": 1,
        "line": line,
        "error": message.to_string(),
    })
}

fn run_jsonl() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut engine = DomEngine::new()?;

    for (line_index, line) in stdin.lock().lines().enumerate() {
        let line_number = line_index + 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let output = match serde_json::from_str::<Value>(&line) {
            Ok(request) => {
                let html = request.get("html").and_then(Value::as_str);
                let url = request
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("about:blank");
                match html {
                    Some(html) => match engine.parse(html, url) {
                        Ok(artifact) => serde_json::to_value(artifact)?,
                        Err(error) => jsonl_error(line_number, error),
                    },
                    None => jsonl_error(line_number, "request requires a string 'html' field"),
                }
            }
            Err(error) => jsonl_error(line_number, format!("invalid request JSON: {error}")),
        };

        serde_json::to_writer(&mut stdout, &output)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

fn main() {
    let result = match parse_arguments() {
        Ok(Some(arguments)) if arguments.jsonl => run_jsonl(),
        Ok(Some(arguments)) => run_single(arguments),
        Ok(None) => return,
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            std::process::exit(2);
        }
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
