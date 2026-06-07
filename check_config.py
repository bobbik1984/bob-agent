import json
with open(r'C:\Users\xm_bo\AppData\Roaming\bob-agent\config.json', 'r', encoding='utf-8') as f:
    config = json.load(f)
    print(f'clerkModel: {config.get("clerkModel", "NOT SET")}')
