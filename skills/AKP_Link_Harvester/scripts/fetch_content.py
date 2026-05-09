#!/usr/bin/env python3
import sys
import requests
from markitdown import MarkItDown

def fetch_text(url):
    try:
        md = MarkItDown()
        
        # 设置请求头
        headers = {
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'
        }
        
        # 针对微信公众号的反爬措施
        if "mp.weixin.qq.com" in url:
            headers['User-Agent'] = 'Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.210 Mobile Safari/537.36 MicroMessenger/8.0.22.2140(0x28001633) WeChat/arm64 Weixin NetType/WIFI Language/zh_CN ABI/arm64'
            headers['Accept'] = 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9'
            
        md._requests_session.headers.update(headers)
        
        result = md.convert(url)
        text = result.text_content
        
        # 针对微信公众号尾部冗余 UI 文本的硬裁剪 (节约 Tokens 和视觉干扰)
        if "mp.weixin.qq.com" in url:
            import re
            # 常见的微信结尾特征词，找到最早出现的那个进行截断
            cutoff_keywords = [
                "微信扫一扫可打开此内容", 
                "微信扫一扫关注该公众号",
                "预览时标签不可点",
                "轻点两下取消赞",
                "轻点两下取消在看",
                "人划线"
            ]
            
            earliest_idx = len(text)
            for keyword in cutoff_keywords:
                idx = text.rfind(keyword)
                if idx != -1 and idx < earliest_idx:
                    earliest_idx = idx
            
            if earliest_idx < len(text):
                text = text[:earliest_idx]
                
            # 清理末尾残留的图片标签和无意义标点
            text = re.sub(r'!\[.*?\]\(.*?\)\s*$', '', text)
            text = re.sub(r'[，。：；,\.:;\n\s]+$', '', text)
        
        # 截取前 30000 字符，Markdown 排版很好所以可以给模型更多上下文，也方便存盘
        return text[:30000]
        
    except Exception as e:
        return f"获取内容失败: {str(e)}"


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("请提供 URL")
        sys.exit(1)
    
    # 解决 Windows 环境下因为控制台编码导致的 gbk 报错
    import io
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    
    url = sys.argv[1]
    content = fetch_text(url)
    print(content)
