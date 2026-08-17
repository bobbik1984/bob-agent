# 搜索策略经验库（从历史执行中学习）
> 最后更新: 2026-04-15 | 来源: CLI 测试 + WebUI 测试

## 航班搜索平台

| 平台 | 搜索域名 | snippet 有价格 | web_fetch 可用 | 推荐度 | 备注 |
|------|---------|--------------|---------------|--------|------|
| Skyscanner 中文 | tianxun.com | ✅ 往返低至¥1,829 | ❌ 动态渲染 | ⭐⭐⭐⭐⭐ | snippet 已包含价格，无需 fetch |
| Skyscanner 国际 | skyscanner.com | ✅ $125起 | ❌ 动态渲染 | ⭐⭐⭐⭐ | 英文搜索时使用 |
| 携程 | flights.ctrip.com | ❌ snippet 无价格 | ❌ 被拦截 | ⭐⭐ | URL 可给用户自行查询 |
| Trip.com | trip.com | ✅ US$120起 | ❌ 动态渲染 | ⭐⭐⭐⭐ | 标题直接含价格 |
| Traveloka | traveloka.com | ❌ snippet 信息少 | 未测试 | ⭐⭐⭐ | 东南亚航线可用 |
| Google Flights | google.com/travel | ❌ 无法搜到详情 | ❌ JS渲染 | ⭐ | 仅作为推荐链接给用户 |

## 最佳搜索策略

### 航班价格
1. **首选关键词**: `"{出发城市} {目的地} 机票 {月份}"` — 中文搜索直接命中天巡/携程
2. **补充关键词**: `"{airport code} {airport code} flight {month} {year}"` — 英文搜索命中国际版
3. **技巧**: DuckDuckGo snippet 中经常直接包含 "低至¥X" 或 "$X起" 的价格，**无需 web_fetch 即可获得参考价**
4. **注意**: 第 1-2 条结果可能是广告（已在 search.py 中过滤）

### 酒店价格
1. **首选**: `"{酒店名} {地区} 价格 评价"` — 直接搜特定酒店
2. **补充**: `"{地区} hotel {月份} site:booking.com"` — 限定 Booking 域名
3. **经验**: Booking.com 页面 web_fetch 部分可用，但价格在动态区域无法提取
4. **建议**: 从 snippet 提取评分和大致价格区间，提供预订链接让用户自查

### 签证政策
1. **首选**: `"中国公民 {国家} 签证 {年份}"` — 中文搜索
2. **补充**: `"Chinese passport {country} visa requirement {year}"` — 英文搜索
3. **经验**: 签证信息在 snippet 中通常足够准确（免签/落地签/需签证等关键信息）
4. **验证**: 如果信息存疑，用 web_fetch 抓取外交部或目的国签证官网

## 常见陷阱

1. **动态渲染**: 几乎所有机票/酒店比价网站都用 JS 动态渲染价格，web_fetch 只能拿到页面框架
2. **反爬机制**: 携程、Agoda 等有积极的反爬措施，web_fetch 大概率返回空内容或验证码
3. **价格过期**: snippet 中的价格可能是缓存的，标注为 🟡 参考值
4. **广告干扰**: DuckDuckGo 前 1-2 条可能是广告（已在 parser 中过滤 duckduckgo.com/y.js 域名）
