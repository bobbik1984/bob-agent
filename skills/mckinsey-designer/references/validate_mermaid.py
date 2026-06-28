"""
Mermaid Diagram Syntax Validator
================================
Validates Mermaid DSL code for common syntax errors before rendering.
Usage:
  python validate_mermaid.py "<mermaid code>"
  python validate_mermaid.py --file diagram.mmd

Returns:
  Exit code 0 + "PASS" on success
  Exit code 1 + "FAIL: <reason>" on failure
"""
import sys
import re
import argparse


def validate_mermaid(code: str) -> tuple[bool, str]:
    """Validate Mermaid code for common syntax errors.
    
    Returns (is_valid, message).
    """
    code = code.strip()
    if not code:
        return False, "Empty diagram code"

    lines = code.split('\n')
    # Strip init frontmatter for validation
    clean_lines = []
    for line in lines:
        stripped = line.strip()
        if stripped.startswith('%%{') or stripped.startswith('%%}'):
            continue
        if stripped.startswith('%%'):
            continue
        clean_lines.append(line)
    
    clean_code = '\n'.join(clean_lines).strip()
    if not clean_code:
        return False, "No diagram content after stripping comments"

    # --- Check 1: Valid diagram type declaration ---
    first_line = clean_code.split('\n')[0].strip().lower()
    valid_starts = [
        'graph ', 'flowchart ', 'sequencediagram', 'classdiagram',
        'statediagram', 'statediagram-v2', 'erdiagram', 'gantt',
        'pie', 'mindmap', 'timeline', 'gitgraph', 'c4context',
        'c4container', 'c4component', 'c4deployment', 'journey',
        'quadrantchart', 'xychart-beta', 'block-beta', 'sankey-beta',
    ]
    has_valid_start = any(first_line.startswith(s) for s in valid_starts)
    if not has_valid_start:
        return False, f"Invalid diagram type. First line: '{first_line}'. Expected one of: {', '.join(valid_starts[:8])}..."

    # --- Check 2: Balanced subgraph/end pairs ---
    subgraph_count = 0
    end_count = 0
    for line in clean_lines:
        stripped = line.strip().lower()
        if stripped.startswith('subgraph ') or stripped == 'subgraph':
            subgraph_count += 1
        if stripped == 'end':
            end_count += 1
    
    if subgraph_count != end_count:
        return False, f"Unbalanced subgraph/end: {subgraph_count} subgraph(s) but {end_count} end(s)"

    # --- Check 3: Dangling arrows ---
    arrow_patterns = [r'-->', r'-\.->', r'==>', r'---', r'-\.->']
    for i, line in enumerate(clean_lines, 1):
        stripped = line.strip()
        if not stripped or stripped.startswith('%%') or stripped.lower().startswith('subgraph') or stripped.lower() == 'end':
            continue
        if stripped.lower().startswith(('graph ', 'flowchart ', 'sequencediagram', 'classdiagram', 'statediagram', 'gantt', 'mindmap', 'style ', 'classDef ', 'class ', 'click ', 'linkStyle ')):
            continue
        # Check for arrows that start or end a line without a node
        for arrow in ['-->', '-.->',  '==>']:
            if stripped.startswith(arrow):
                return False, f"Line {i}: Dangling arrow at start: '{stripped}'"
            if stripped.endswith(arrow):
                return False, f"Line {i}: Dangling arrow at end: '{stripped}'"

    # --- Check 4: Unquoted special characters in node labels ---
    # Look for node definitions with brackets containing special chars but no quotes
    bracket_pattern = re.compile(r'\[([^\]"]+)\]')
    special_chars = set('(){}|<>&')
    for i, line in enumerate(clean_lines, 1):
        stripped = line.strip()
        for match in bracket_pattern.finditer(stripped):
            label = match.group(1)
            if any(c in label for c in special_chars):
                return False, f"Line {i}: Node label contains special characters without quotes: [{label}]. Use [\"{label}\"] instead."

    # --- Check 5: Basic structure validation for sequence diagrams ---
    if first_line.startswith('sequencediagram'):
        for i, line in enumerate(clean_lines[1:], 2):
            stripped = line.strip()
            if not stripped or stripped.startswith('%%') or stripped.lower().startswith(('participant', 'actor', 'note', 'loop', 'alt', 'opt', 'par', 'rect', 'end', 'activate', 'deactivate', 'title')):
                continue
            if '->>' not in stripped and '-->>>' not in stripped and '->' not in stripped and '-->' not in stripped and '-x' not in stripped and '--)' not in stripped:
                if not stripped.startswith(('note ', 'end', 'loop', 'alt', 'opt', 'par', 'rect', 'break', 'critical')):
                    pass  # Allow unknown lines in sequence diagrams (could be labels)

    return True, "PASS"


def main():
    parser = argparse.ArgumentParser(description='Validate Mermaid diagram syntax')
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('code', nargs='?', help='Mermaid code as string')
    group.add_argument('--file', '-f', help='Path to .mmd file')
    args = parser.parse_args()

    if args.file:
        with open(args.file, 'r', encoding='utf-8') as f:
            code = f.read()
    else:
        code = args.code

    is_valid, message = validate_mermaid(code)
    
    if is_valid:
        print("PASS")
        sys.exit(0)
    else:
        print(f"FAIL: {message}")
        sys.exit(1)


if __name__ == '__main__':
    main()
