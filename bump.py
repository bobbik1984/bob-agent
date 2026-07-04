import re

def update_file(path, pattern, repl):
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    new_content = re.sub(pattern, repl, content)
    if new_content != content:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Updated {path}")

# package.json
update_file('package.json', r'"version": "0.5.1"', '"version": "0.5.2"')

# tauri.conf.json
update_file('src-tauri/tauri.conf.json', r'"version": "0.5.1"', '"version": "0.5.2"')

# Cargo.toml
update_file('src-tauri/Cargo.toml', r'version = "0.5.1"', 'version = "0.5.2"')

try:
    update_file('installer/src-tauri/Cargo.toml', r'version = "0.5.1"', 'version = "0.5.2"')
except FileNotFoundError:
    pass

try:
    update_file('installer/src-tauri/tauri.conf.json', r'"version": "0.5.1"', '"version": "0.5.2"')
except FileNotFoundError:
    pass
