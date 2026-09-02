// trueos-blueprint: features=["trueos-first"]

mod page_script;
mod parser_probe;

fn main() {
    if let Err(error) = parser_probe::run() {
        parser_probe::report_error(format_args!("solara: browser probe failed: {error}"));
    }
}
