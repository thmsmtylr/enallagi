fn main() {
    println!("cargo:rerun-if-changed=adapters/presets");
    println!("cargo:rerun-if-changed=runners");
}
