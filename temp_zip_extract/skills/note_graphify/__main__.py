"""note_graphify CLI 入口 — 从 iknow 根目录运行"""
import sys
sys.path.insert(0, str(__import__("pathlib").Path(__file__).parent))
from scripts.note_graphify import main
main()
