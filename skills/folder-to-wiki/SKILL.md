---
name: folder-to-wiki
description: |
  灏嗘湰鍦版枃浠跺す鏀跺綍杩?LLM-Wiki 鐭ヨ瘑搴撱傛壂鎻忕洰褰?鈫?閫愭枃浠舵彁鍙栨枃鏈?鈫?璇箟鍘嬬缉涓虹粨鏋勫寲鎽樿 鈫?鍐欏叆 wiki/sources/ 骞舵洿鏂?index.md 鍏ㄥ眬鐩綍銆?  瑙﹀彂璇嶏細鏀跺綍鐩綍銆佹敹褰曟枃浠跺す銆佹暣鐞嗗埌鐭ヨ瘑搴撱佹妸XXX鍔犲叆鐭ヨ瘑搴撱乮ngest folder銆乥uild wiki銆?  鈿狅笍 鏈妧鑳介潰鍚戝閮?Agent (Antigravity/Claude Code/CodeRunner) 浣跨敤銆侭ob-Agent 鏈夊唴鍖栫殑 Rust 绠￠亾锛坘b_extractor + kb_indexer锛夛紝鏃犻渶姝ゆ妧鑳姐?---

# folder-to-wiki 鈥?鏂囦欢澶?鈫?LLM-Wiki 鏀跺綍鎶鑳?
## 鍓嶇疆鐭ヨ瘑

LLM-Wiki 鏄竴绉嶆寔涔呭寲鐨勩佺敱 LLM 缁存姢鐨?Markdown 鐭ヨ瘑鍥捐氨銆備笌浼犵粺 RAG锛堟瘡娆′粠纰庣墖涓绱級涓嶅悓锛孡LM-Wiki 棰勫厛灏嗘枃浠跺唴瀹?*缂栫粐**鎴愮粨鏋勫寲鎽樿椤碉紝褰㈡垚涓妫靛彲瀵艰埅鐨勭煡璇嗘爲銆?
鍙傝冩灦鏋勶細`wiki/llm-wiki.md`

## Wiki 鐩綍缁撴瀯

```
{wiki_root}/
鈹溾攢鈹 index.md          鈫?鍏ㄥ眬鐩綍 (鎵鏈?source 椤甸潰鐨勯摼鎺?+ 涓鍙ヨ瘽鎽樿)
鈹溾攢鈹 log.md            鈫?鎿嶄綔鏃ュ織 (append-only锛岃褰曟瘡娆?ingest)
鈹溾攢鈹 sources/          鈫?姣忎釜鏂囦欢鐨勭粨鏋勫寲鎽樿椤?鈹?  鈹溾攢鈹 file_a.md
鈹?  鈹斺攢鈹 file_b.md
鈹溾攢鈹 entities/         鈫?璺ㄦ枃浠舵彁鍙栫殑瀹炰綋/姒傚康椤?鈹?  鈹溾攢鈹 entity_x.md
鈹?  鈹斺攢鈹 entity_y.md
鈹斺攢鈹 projects/         鈫?鎸夋枃浠跺す绮掑害鐨勭患杩伴〉
    鈹斺攢鈹 folder_name.md
```

## Wiki 鏍圭洰褰?
**榛樿璺緞**: `wiki/`

濡傛灉鐢ㄦ埛鎸囧畾浜嗗叾浠栬矾寰勶紝浠ョ敤鎴锋寚瀹氱殑涓哄噯銆?
## 鎵ц娴佺▼

### Step 1: 鎵弿鐩綍

浣跨敤 `list_dir` 鎴栫郴缁熷懡浠ら掑綊鎵弿鐩爣鏂囦欢澶癸紝鐢熸垚鏂囦欢娓呭崟銆?
**杩囨护瑙勫垯**:
- 鉁?鏀跺綍: `.txt`, `.md`, `.csv`, `.json`, `.yaml`, `.xml`, `.html`, `.pdf`, `.docx`, `.pptx`, `.xlsx`, `.xls`, `.log`, `.ini`, `.cfg`, `.rst`, `.tex`
- 鈴笍 璺宠繃鐩綍: `node_modules`, `target`, `dist`, `build`, `__pycache__`, `.git`, `venv`, `.venv`, 浠?`.` 寮澶寸殑闅愯棌鐩綍
- 鈴笍 璺宠繃鏂囦欢: >50MB 鐨勮秴澶ф枃浠?- 馃摳 鍥剧墖/闊宠棰? 浠呰褰曟枃浠跺悕浣滀负璇箟鍗犱綅绗?`[鍥剧墖: xxx.jpg]`

### Step 2: 閫愭枃浠舵彁鍙栨枃鏈?
瀵规瘡涓彲澶勭悊鐨勬枃浠讹細
- **绾枃鏈?* (.txt, .md, .csv, .json 绛?: 鐩存帴 `read_file`
- **PDF/DOCX/PPTX/XLSX**: 濡傛灉鏈?`markitdown` 宸ュ叿鍒欎娇鐢紝鍚﹀垯灏濊瘯 `read_file`锛堝彲鑳芥湁涔辩爜锛屾爣娉ㄥ嵆鍙級
- **鎴彇涓婇檺**: 姣忎釜鏂囦欢鏈澶氭彁鍙栧墠 8000 涓瓧绗︾敤浜庢憳瑕?
### Step 3: 璇箟鍘嬬缉

瀵规彁鍙栫殑鏂囨湰鐢熸垚缁撴瀯鍖栨憳瑕併傚彲浠ョ洿鎺ョ敤褰撳墠瀵硅瘽鐨?LLM 鑳藉姏锛圓ntigravity/Claude Code 鏈韩灏辨槸楂樿川閲忔ā鍨嬶級锛屾棤闇棰濆 API 璋冪敤銆?
**杈撳嚭鏍煎紡** (姣忎釜鏂囦欢):
```json
{
  "summary": "涓嶈秴杩?300 瀛楃殑鏍稿績鍐呭鎽樿",
  "keywords": ["鍏抽敭璇?", "鍏抽敭璇?", "鍏抽敭璇?"],
  "entities": [
    {"name": "瀹炰綋鍚嶇О", "type": "浜虹墿|缁勭粐|姒傚康|鍦扮偣|鏀跨瓥", "description": "涓鍙ヨ瘽鎻忚堪"}
  ],
  "data_points": ["鍏抽敭鏁版嵁鐐?", "鍏抽敭鏁版嵁鐐?"]
}
```

### Step 4: 鍐欏叆 Wiki

#### 4a. 鍐欏叆 Source 椤甸潰

璺緞: `{wiki_root}/sources/{safe_filename}.md`

```markdown
---
source: {鍘熷鏂囦欢缁濆璺緞}
type: {鏂囦欢绫诲瀷 pdf/docx/text/xlsx}
tags: [鍏抽敭璇?, 鍏抽敭璇?, 鍏抽敭璇?]
indexed_at: {褰撳墠鏃堕棿 YYYY-MM-DD HH:MM}
---

# {鏂囦欢鍚?涓嶅惈鎵╁睍鍚?}

## 鎽樿

{300瀛楁憳瑕亇

## 鍏抽敭鏁版嵁鐐?
- {鏁版嵁鐐?}
- {鏁版嵁鐐?}
```

> **鈿狅笍 閾佸緥**: `source` 瀛楁蹇呴』浣跨敤**缁濆璺緞**锛堝 `C:\Users\Username\Documents\report.pdf`锛夛紝涓嶅緱浣跨敤鐩稿璺緞銆?
#### 4b. 鍐欏叆 Entity 椤甸潰

璺緞: `{wiki_root}/entities/{entity_name}.md`

濡傛灉瀹炰綋椤靛凡瀛樺湪锛?*杩藉姞**"鐩稿叧鏂囦欢"閾炬帴锛屼笉瑕嗙洊銆?
#### 4c. 鏇存柊 index.md

鍦?`## Sources` 娈佃惤涓嬭拷鍔犱竴琛岋細
```
- [{鏂囦欢鍚峿](sources/{safe_filename}.md) 鈥?{涓鍙ヨ瘽鎽樿(80瀛楀唴)}
```

#### 4d. 鏇存柊 log.md

杩藉姞涓鏉¤褰曪細
```
## [{鏃堕棿}] ingest | {鏂囦欢鍚峿 鈫?鉁?{涓鍙ヨ瘽鎽樿}
```

#### 4e. 鐢熸垚 Project 缁艰堪椤?
璺緞: `{wiki_root}/projects/{folder_name}.md`

姹囨绘湰娆℃敹褰曠殑鎵鏈夋枃浠讹紝鐢熸垚缁艰堪椤碉紝骞跺湪 `index.md` 鐨?`## Projects` 娈佃惤涓嬫坊鍔犻摼鎺ャ?
## 鎵归噺澶勭悊绛栫暐

- 鏂囦欢鏁?鈮?10: 閫愭枃浠跺鐞嗭紝姣忎釜鏂囦欢鍗曠嫭杈撳嚭鎽樿
- 鏂囦欢鏁?> 10: 鍒嗘壒澶勭悊锛屾瘡鎵?5-8 涓枃浠讹紝閬垮厤涓婁笅鏂囨孩鍑?- 姣忔壒澶勭悊瀹岀珛鍗冲啓鍏?Wiki锛屼笉瑕佺瓑鍏ㄩ儴瀹屾垚

## 涓?Bob-Agent 鍐呭寲绠￠亾鐨勫叧绯?
Bob-Agent 鐨?Rust 绠￠亾 (`kb_extractor.rs` + `kb_indexer.rs`) 鍜屾湰鎶鑳藉啓鍏ョ殑鏄?*瀹屽叏鐩稿悓鐨?Wiki 缁撴瀯**銆備袱鑰呬骇鍑虹殑 source 椤甸潰鍙互鍏卞瓨浜掕ˉ锛?
- Bob 澶勭悊杩囩殑鏂囦欢 鈫?wiki/sources/xxx.md (by Clerk)
- 澶栭儴 Agent 澶勭悊杩囩殑鏂囦欢 鈫?wiki/sources/yyy.md (by this skill)
- index.md 缁熶竴绱㈠紩涓よ?
## 甯歌瑙﹀彂鍦烘櫙

鐢ㄦ埛璇?| 鍔ㄤ綔
--- | ---
"鎶?C:\Users\Username\Documents\XXX 鏀跺綍杩涚煡璇嗗簱" | 鎵ц瀹屾暣娴佺▼
"鏁寸悊杩欎釜鐩綍鍒?wiki" | 鎵ц瀹屾暣娴佺▼
"杩欎釜鏂囦欢澶规湁浠涔堝唴瀹癸紵" | 鍙墽琛?Step 1-2锛堟壂鎻?棰勮锛夛紝涓嶅啓鍏?"鏇存柊鐭ヨ瘑搴撶储寮? | 鍙墽琛?Step 4c锛堥噸鏂扮敓鎴?index.md锛?
