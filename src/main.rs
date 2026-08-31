// trueos-blueprint: features=["trueos-first"]

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn main() {
    println!("solara: TRUEOS-first inert host placeholder (no browser or renderer)");
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn main() {
    trueos::logl::log(
        trueos::logl::level::INFO,
        "solara: TRUEOS-first inert entry; no Frame, text rows, shapes, or presentation",
    );
}
