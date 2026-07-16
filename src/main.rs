// trueos-blueprint: features=["ui4-solara-text"]
// Blueprint build-plan marker; the target-aware attribute below is the active #![no_std] policy.
#![cfg_attr(any(target_os = "trueos", target_os = "zkvm"), no_std)]

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod gpu_ui;
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
mod trueos_app;

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
const DEFAULT_LOG_FILTER: &str = "warn,sctk_adwaita::buttons=error";

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn main() {
    let mut logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(DEFAULT_LOG_FILTER),
    );
    // sctk-adwaita 0.10 warns when GNOME supplies a valid empty left
    // title-bar button list. Keep that dependency bug quiet even when a
    // workspace-wide RUST_LOG=warn overrides the default filter.
    logger.parse_filters("sctk_adwaita::buttons=error");
    let _ = logger.try_init();
    let source = std::env::args().nth(1);
    if let Err(error) = gpu_ui::run(source) {
        eprintln!("solara: {error}");
        std::process::exit(1);
    }
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn main() {
    trueos_app::run();
}
