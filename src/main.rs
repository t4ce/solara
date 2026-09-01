// trueos-blueprint: features=["trueos-first"]

mod parser_probe;

fn main() {
    if let Err(error) = parser_probe::run() {
        parser_probe::report_error(format_args!("solara: five-page parse failed: {error}"));
    }
}
