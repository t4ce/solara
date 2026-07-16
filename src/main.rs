mod gpu_ui;

fn main() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .try_init();
    let source = std::env::args().nth(1);
    if let Err(error) = gpu_ui::run(source) {
        eprintln!("solara: {error}");
        std::process::exit(1);
    }
}
