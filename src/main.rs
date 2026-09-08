// trueos-blueprint: features=["trueos-first"]

#[cfg(feature = "spec-layout")]
mod layout_probe;
#[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
mod native_scripts;
#[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
mod native_images;
#[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
mod native_favicon;
#[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
mod native_tui;
#[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
mod native_window;
mod page_script;
mod parser_probe;
mod run_script;

fn main() {
    if let Err(error) = run() {
        parser_probe::report_error(format_args!("solara: browser probe failed: {error}"));
    }
}

fn run() -> Result<(), String> {
    #[cfg(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm")))]
    {
        #[cfg(feature = "native-demos")]
        return native_window::run(None);
        #[cfg(not(feature = "native-demos"))]
        return native_window::run_browser();
    }
    #[cfg(not(all(feature = "spec-layout", any(target_os = "trueos", target_os = "zkvm"))))]
    {
        let Some(request) = run_script::read()? else {
            return parser_probe::run();
        };
        let source = request
            .source
            .ok_or("host probe requires a staged source file")?;
        let html = read_source(&source)?;
        parser_probe::run_page(request.url.as_str(), &html)
    }
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn read_source(path: &str) -> Result<String, String> {
    trueos::async_fs::block_on(trueos::async_fs::read_file_utf8(path.as_bytes()))
        .map_err(|error| format!("could not read surf source {path:?}: error {error}"))
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn read_source(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("could not read surf source {path:?}: {error}"))
}
