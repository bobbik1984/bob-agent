import json
import os

def update_locale(path, lang):
    with open(path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    if "ticket" not in data:
        data["ticket"] = {}
        
    if lang == "zh":
        data["ticket"].update({
            "passenger": "乘客",
            "flight": "航班",
            "date": "日期",
            "time": "时间",
            "seat": "座位",
            "pnr": "PNR",
            "venue": "场馆"
        })
    else:
        data["ticket"].update({
            "passenger": "Passenger",
            "flight": "Flight",
            "date": "Date",
            "time": "Time",
            "seat": "Seat",
            "pnr": "PNR",
            "venue": "Venue"
        })
        
    with open(path, 'w', encoding='utf-8') as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

update_locale('src/locales/zh-CN.json', 'zh')
update_locale('src/locales/en-US.json', 'en')
print("Locales updated.")
