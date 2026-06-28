import os

def update_version(filepath, old_version, new_version, old_v_text, new_v_text):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Replace the query param version
    content = content.replace(f"?v={old_version}", f"?v={new_version}")
    
    # Replace the text version
    content = content.replace(f"(v{old_v_text})", f"(v{new_v_text})")
    
    # Replace CSS version
    content = content.replace('style.css?v=1.5.2', 'style.css?v=1.5.3')
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(content)
    
    print(f"Updated {filepath}")

update_version('website/index.html', '0.4.0.1', '0.4.2', '0.4.0', '0.4.2')
update_version('website/index_en.html', '0.4.0.1', '0.4.2', '0.4.0', '0.4.2')
