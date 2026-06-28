#!/bin/bash
# evening_review.sh - 晚间总结脚本
# 此脚本用于将大模型生成的“今日总结”写入全局的 daily 目录中。
# 用法: cat summary.md | ./evening_review.sh

WORKSPACE_DIR="/home/ubuntu/.openclaw/workspace"
COMMON_DIR="$WORKSPACE_DIR/common"
DAILY_DIR="$WORKSPACE_DIR/daily"
# To support local development dynamically:
if [ ! -d "$WORKSPACE_DIR" ] && [ -d "/opt/syncthing" ]; then
    WORKSPACE_DIR="/opt/syncthing"
fi
# if running locally, use relative path or local absolute
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
    # Win dev environment mapping
    DAILY_DIR="$(cd "$(dirname "$0")/../../../../daily" && pwd)"
fi

mkdir -p "$DAILY_DIR"

TODAY_DATE=$(date '+%Y-%m-%d')
SUMMARY_FILE="$DAILY_DIR/$TODAY_DATE.md"

echo "Saving evening review to $SUMMARY_FILE..."

# 读取标准输入并写入文件 (覆盖写入，保证幂等性)
cat > "$SUMMARY_FILE"

echo "Successfully saved daily summary for $TODAY_DATE."
echo "You can now safely shut down your context for the day."
