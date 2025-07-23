根据Makefile，启动项目前的完整setup操作如下：

### 1. 安装hoonc
首先需要安装hoonc工具，这是编译Hoon文件为jam文件的必要工具：
```bash
cd nockchain-miningpool
make install-hoonc ✅
```

### 2. 生成必要的jam文件
虽然后续步骤会自动触发这个过程，但也可以单独执行确保所有jam文件正确生成：
```bash
make build-hoon-all ✅
```
这一步会生成三个关键的jam文件：`assets/dumb.jam`、`assets/miner.jam`和`assets/wal.jam`

### 3. 安装所有必要的组件
分别安装三个核心组件：

```bash
# 安装矿工客户端(nockchain)
make install-nockchain ✅

# 安装钱包工具
make install-nockchain-wallet

# 编译矿池服务器
# 注意：没有直接的install-pool-server目标，但可以通过以下方式安装
make pool-server
cargo install --locked --force --path crates/nockchain-pool-server --bin pool-server
```

### 4. 准备分发包（可选）
如果您需要准备分发包，可以使用：
```bash
make dist
```
这将在dist目录下创建一个包含所有必要可执行文件和配置示例的分发包。

### 总结：最小化的安装流程
如果要用最少的命令完成整个setup，可以执行：
```bash
# 1. 安装hoonc
make install-hoonc

# 2. 安装矿工客户端和钱包工具（这会自动触发build-hoon-all）
make install-nockchain
make install-nockchain-wallet

# 3. 安装矿池服务器
cargo install --locked --force --path crates/nockchain-pool-server --bin pool-server
```

这样就完成了所有必要组件的安装，接下来就可以按照README中的说明配置和启动系统了。