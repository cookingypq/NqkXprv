use std::error::Error;
use vergen::EmitBuilder;

fn main() -> Result<(), Box<dyn Error>> {
    // 使用新的vergen 8.x API
    // Using new vergen 8.x API
    EmitBuilder::builder()
        .git_sha(true)
        .git_commit_timestamp()
        .emit()?;
    
    // 新增编译protobuf文件为Rust代码 | Add compilation of protobuf file to Rust code
    // 仅当启用了矿池模式特性时才编译proto文件 | Only compile proto files when pool mode feature is enabled
    #[cfg(feature = "pool_client")]
    tonic_build::compile_protos("proto/pool.proto")?;
    
    #[cfg(feature = "pool_client")]
    println!("cargo:rerun-if-changed=proto/pool.proto");
    
    Ok(())
}
