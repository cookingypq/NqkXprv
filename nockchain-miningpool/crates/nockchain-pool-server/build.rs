fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 编译protobuf文件为Rust代码 | Compile protobuf file to Rust code
    tonic_build::compile_protos("proto/pool.proto")?;
    println!("cargo:rerun-if-changed=proto/pool.proto");
    Ok(())
} 