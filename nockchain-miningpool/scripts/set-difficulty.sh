#!/bin/bash
# 一键设置区块难度
# 用法: ./set-difficulty.sh <难度等级数字>
# 例如: ./set-difficulty.sh 1 代表2^1，2代表2^2，依此类推，最大支持40
LEVEL="$1"
ENV_FILE="$(dirname "$0")/../.env"
if [[ -z "$LEVEL" ]]; then
    echo "用法: $0 <难度等级数字> (1~40)"
    exit 1
fi
if (( LEVEL < 1 || LEVEL > 40 )); then
    echo "难度等级范围为1~40，当前输入: $LEVEL"
    exit 1
fi
# 计算难度目标
# 目标为64字节（128位16进制），最高有效位为1，其余为0，后面全ff
# 例如 LEVEL=1 => 0x4000...ff，LEVEL=2 => 0x2000...ff，LEVEL=8 => 0x0100...ff，LEVEL=9 => 0x0080...ff
# 以bit为单位左移，最高位在第一个字节
let bitpos=320-LEVEL  # 320=64*5，256位+64位预留
DIFFICULTY_HEX=$(printf '%0128s' | tr ' ' 'f')
# 计算需要设置1的字节和bit
let byte_index=bitpos/8
let bit_index=bitpos%8
# 生成初始全ff的数组
DIFFICULTY_ARR=()
for ((i=0;i<64;i++)); do
    DIFFICULTY_ARR+=("ff")
done
# 设置目标位
HEX_VAL=$((0x80 >> bit_index))
DIFFICULTY_ARR[$byte_index]=$(printf "%02x" $HEX_VAL)
# 拼接
DIFFICULTY_TARGET=$(printf "%s" "${DIFFICULTY_ARR[@]}")
# 写入 .env
if grep -q "^DIFFICULTY_TARGET=" "$ENV_FILE"; then
    sed -i '' "s/^DIFFICULTY_TARGET=.*/DIFFICULTY_TARGET=$DIFFICULTY_TARGET/" "$ENV_FILE"
else
    echo "DIFFICULTY_TARGET=$DIFFICULTY_TARGET" >> "$ENV_FILE"
fi
echo "已设置区块难度为: $DIFFICULTY_TARGET (等级$LEVEL)"
echo "请重启 pool-server 以生效。" 