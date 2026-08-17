#!/bin/bash
# morning_kickoff.sh - 晨间启动脚本
# 此脚本用于读取昨日的总结、项目蓝图和全局任务，将其合并输出到标准输出，
# 并供大模型读取后生成或重置今天的 `today_plan.md`。

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
    PROJECT_DIR="$(cd "$(dirname "$0")/../../../../.." && pwd)"
else
    PROJECT_DIR="/home/ubuntu/.openclaw/workspace/common" # default fallback
fi

# 1. 寻找最近的一份 Summary
YESTERDAY_DATE=$(date -d "yesterday" '+%Y-%m-%d' 2>/dev/null || date -v-1d '+%Y-%m-%d' 2>/dev/null)
SUMMARY_FILE="$DAILY_DIR/$YESTERDAY_DATE.md"

if [ ! -f "$SUMMARY_FILE" ]; then
    # 尝试寻找最新的文件
    SUMMARY_FILE=$(ls -t "$DAILY_DIR"/20*.md 2>/dev/null | head -1)
fi

echo "========== MORNING KICKOFF CONTEXT =========="
echo ""

echo "--- [1] LAST DAILY SUMMARY ---"
if [ -f "$SUMMARY_FILE" ]; then
    echo "Found summary: $(basename "$SUMMARY_FILE")"
    cat "$SUMMARY_FILE"
else
    echo "No recent daily summary found."
fi
echo ""

# 2. 查找 Project_Blueprint.md 
echo "--- [2] PROJECT BLUEPRINT / IMPLEMENTATION ---"
# Check up the hierarchy for the markdown files
BLUEPRINT_FILE=$(find "$PROJECT_DIR" -maxdepth 2 -name "Project_Blueprint.md" -o -name "Implementation.md" | head -1)

if [ -f "$BLUEPRINT_FILE" ]; then
    # to avoid blowing up context, just grab the first 300 lines or the recent phase
    cat "$BLUEPRINT_FILE" | head -n 200
else
    echo "Blueprint missing in $PROJECT_DIR"
fi
echo ""

# 3. 查找 task.md
echo "--- [3] GLOBAL TASKS (task.md) ---"
TASK_FILE=$(find "$PROJECT_DIR" -maxdepth 2 -name "task.md" | head -1)

if [ -f "$TASK_FILE" ]; then
    cat "$TASK_FILE" | grep -A 50 "Current Status" # Show current status and recent tasks
    # Also show all unchecked tasks:
    echo "--- UNCOMPLETED TASKS ---"
    grep -E "^\s*- \[\s\]" "$TASK_FILE"
else
    echo "Task file missing."
fi

echo ""
echo "============================================="
echo "INSTRUCTION TO ASSISTANT:"
echo "Based on the above context, please write a brief morning plan to 'workspace/daily/today_plan.md' and welcome the user to a new day of work."
