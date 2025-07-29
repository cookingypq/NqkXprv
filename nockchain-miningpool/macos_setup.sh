#!/bin/bash

# 创建 assets 目录
mkdir -p assets

# 安装 hoonc
make install-hoonc

# 构建所有 hoon 文件
make build-hoon-all

# 安装 protobuf 编译器
cargo install protobuf-codegen

# 完成池的构建
make pool-full