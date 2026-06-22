mod gpu_ui;

fn main() {
    let source = std::env::args().nth(1);
    if let Err(error) = gpu_ui::run(source) {
        eprintln!("solara: {error}");
        std::process::exit(1);
    }
}
