use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["proto/engine.proto"], &["proto/"])?;
    Ok(())
}
