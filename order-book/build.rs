use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(&["../proto/order.proto"], &["../proto"])?;
    println!("compiled protos");
    Ok(())
}
