use std::path::Path;

fn main() {
    let after_load_rs = Path::new("src/after_load.rs");
    println!("cargo:rerun-if-changed={}", after_load_rs.display());
    if after_load_rs.exists() {
        println!("cargo:warning=Using custom AfterLoadCmd");
        println!("cargo:rustc-cfg=custom_after_load");
    }
}
