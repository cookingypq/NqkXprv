fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    tonic_build::configure()
        .build_server(false) // 我们只构建客户端
        .compile(
            &["../nockchain-pool-server/proto/pool.proto"], // 修正的 proto 文件路径
            &["../nockchain-pool-server/proto"], // 修正的 proto 文件包含路径
        )
        .unwrap();
}
