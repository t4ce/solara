mod gpu_ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "gpu-ui") {
        gpu_ui::run();
        return;
    }

    println!(
        r#"
   _____       __
  / ___/____  / /___ __________ _
  \__ \/ __ \/ / __ `/ ___/ __ `/
 ___/ / /_/ / / /_/ / /  / /_/ /
/____/\____/_/\__,_/_/   \__,_/

        :: SOLARA ::
   Rust + QuickJS browser lab

Run the GPU UI demo:
  cargo run -- gpu-ui
        "#
    );
}
