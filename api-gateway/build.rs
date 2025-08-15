// Repeating myself for now?
use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(&["../proto/trading.proto"], &["../proto"])?;
    println!("compiled protos");
    Ok(())
}
