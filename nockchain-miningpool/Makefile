# Nockchain矿池套件Makefile
# Nockchain Mining Pool Suite Makefile

# 编译目标平台 | Compile target platform
TARGET = x86_64-unknown-linux-gnu

# 输出目录 | Output directory
OUT_DIR = target/release

# 设置默认目标 | Set default target
.PHONY: all
all: build

# 开发模式构建 | Development mode build
.PHONY: dev
dev: build-hoon-all
	@echo "构建开发版本... | Building development version..."
	@cargo build --workspace

# 构建所有二进制文件（debug模式） | Build all binaries (debug mode)
.PHONY: build
build: build-hoon-all
	@echo "构建所有组件... | Building all components..."
	@cargo build --workspace

# 发布构建 | Release build
.PHONY: release
release: build-hoon-all
	@echo "构建发布版本... | Building release version..."
	@cargo build --release --workspace

# 构建矿池服务器 | Build pool server
.PHONY: pool-server
pool-server:
	@echo "构建矿池服务器... | Building pool server..."
	@cargo build --release -p nockchain-pool-server

# 构建矿工（矿池模式） | Build miner (pool mode)
.PHONY: miner
miner: build-hoon-all
	@echo "构建矿工（矿池模式）... | Building miner (pool mode)..."
	@cargo build --release -p nockchain --features pool_client

# 构建钱包工具 | Build wallet tool
.PHONY: wallet
wallet: build-hoon-all
	@echo "构建钱包工具... | Building wallet tool..."
	@cargo build --release -p nockchain-wallet

# 准备分发包 | Prepare distribution package
.PHONY: dist
dist: release
	@echo "准备分发包... | Preparing distribution package..."
	@mkdir -p dist
	@cp $(OUT_DIR)/pool-server dist/
	@cp $(OUT_DIR)/miner dist/
	@cp $(OUT_DIR)/nockchain-wallet dist/
	@cp crates/nockchain-pool-server/.env_example dist/.env_example
	@cp README.md dist/
	@echo "分发包已准备好，在dist目录下 | Distribution package ready in dist directory"
	@ls -la dist

# 清理编译产物 | Clean build artifacts
.PHONY: clean
clean:
	@echo "清理编译产物... | Cleaning build artifacts..."
	@cargo clean

# 交叉编译 | Cross compilation
.PHONY: cross-build
cross-build:
	@echo "交叉编译发布版本到 $(TARGET)... | Cross-compiling release version to $(TARGET)..."
	@rustup target add $(TARGET)
	@cargo build --release --target $(TARGET) --workspace

# 测试 | Test
.PHONY: test
test:
	@echo "运行测试... | Running tests..."
	@cargo test --workspace

# 帮助 | Help
.PHONY: help
help:
	@echo "Nockchain矿池套件构建系统 | Nockchain Mining Pool Suite Build System"
	@echo ""
	@echo "可用命令: | Available commands:"
	@echo "  make            - 构建所有组件（debug模式）| Build all components (debug mode)"
	@echo "  make dev        - 构建开发版本 | Build development version"
	@echo "  make release    - 构建发布版本 | Build release version"
	@echo "  make pool-server - 只构建矿池服务器 | Build only pool server"
	@echo "  make miner      - 只构建矿工客户端 | Build only miner client"
	@echo "  make wallet     - 只构建钱包工具 | Build only wallet tool"
	@echo "  make dist       - 准备分发包 | Prepare distribution package"
	@echo "  make clean      - 清理编译产物 | Clean build artifacts"
	@echo "  make test       - 运行测试 | Run tests"
	@echo "  make build-hoon-all - 构建所有Hoon文件 | Build all Hoon files"
	@echo "  make help       - 显示帮助信息 | Show help information"

# ======================== Hoon编译相关规则 | Hoon compilation rules ========================

# 确保目录存在 | Ensure directories exist
.PHONY: ensure-dirs
ensure-dirs:
	@echo "确保必要的目录存在... | Ensuring necessary directories exist..."
	@mkdir -p assets

# Hoon目标文件 | Hoon target files
HOON_TARGETS=assets/dumb.jam assets/wal.jam assets/miner.jam

# 清理assets | Clean assets
.PHONY: nuke-assets
nuke-assets:
	@echo "清理assets目录... | Cleaning assets directory..."
	@rm -f assets/*.jam

# 构建所有Hoon文件 | Build all Hoon files
.PHONY: build-hoon-all
build-hoon-all: nuke-assets ensure-dirs $(HOON_TARGETS)
	@echo "所有Hoon文件构建完成 | All Hoon files built"

# 获取Hoon源文件 | Get Hoon source files
HOON_SRCS := $(shell find hoon -type f -name "*.hoon" 2>/dev/null)

# 编译dumb.jam | Build dumb.jam
assets/dumb.jam: hoon/apps/dumbnet/outer.hoon $(HOON_SRCS)
	@echo "编译dumb.jam... | Building dumb.jam..."
	@rm -f assets/dumb.jam
	@hoonc hoon/apps/dumbnet/outer.hoon hoon
	@mv out.jam assets/dumb.jam

# 编译wal.jam | Build wal.jam
assets/wal.jam: hoon/apps/wallet/wallet.hoon $(HOON_SRCS)
	@echo "编译wal.jam... | Building wal.jam..."
	@rm -f assets/wal.jam
	@hoonc hoon/apps/wallet/wallet.hoon hoon
	@mv out.jam assets/wal.jam

# 编译miner.jam | Build miner.jam
assets/miner.jam: hoon/apps/dumbnet/miner.hoon $(HOON_SRCS)
	@echo "编译miner.jam... | Building miner.jam..."
	@rm -f assets/miner.jam
	@hoonc hoon/apps/dumbnet/miner.hoon hoon
	@mv out.jam assets/miner.jam

# 安装hoonc | Install hoonc
.PHONY: install-hoonc
install-hoonc:
	@echo "安装hoonc... | Installing hoonc..."
	@cargo install --locked --path crates/hoonc --bin hoonc

# 安装矿工客户端 | Install miner client
.PHONY: install-nockchain
install-nockchain: assets/dumb.jam assets/miner.jam
	@echo "安装nockchain（矿工客户端）... | Installing nockchain (miner client)..."
	@cargo install --locked --force --path crates/nockchain --bin miner --features pool_client

# 安装钱包工具 | Install wallet tool
.PHONY: install-nockchain-wallet
install-nockchain-wallet: assets/wal.jam
	@echo "安装nockchain钱包... | Installing nockchain wallet..."
	@cargo install --locked --force --path crates/nockchain-wallet --bin nockchain-wallet
