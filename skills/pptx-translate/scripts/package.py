#!/usr/bin/env python3
"""
package_skill.py — Package the pptx-translate skill into a .skill file (zip).
Run from the parent directory of pptx-translate/.
"""
import shutil
import sys
from pathlib import Path

skill_dir = Path(__file__).parent
output = Path("/mnt/user-data/outputs/pptx-translate.skill")
output.parent.mkdir(parents=True, exist_ok=True)

shutil.make_archive(str(output.with_suffix("")), "zip", skill_dir.parent, skill_dir.name)
Path(str(output.with_suffix("")) + ".zip").rename(output)
print(f"Packaged: {output}")
