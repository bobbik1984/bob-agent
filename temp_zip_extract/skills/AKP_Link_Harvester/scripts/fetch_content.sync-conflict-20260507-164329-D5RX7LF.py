#!/usr/bin/env python3
import sys
import requests
from bs4 import BeautifulSoup


def fetch_text(url):
    try:
        headers = {
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'
        }
        # 针对微信公众号的反爬措施，单独伪装为微信客户端（免截图、无显卡环境适用）
        if "mp.weixin.qq.com" in url:
            headers['User-Agent'] = 'Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.210 Mobile Safari/537.36 MicroMessenger/8.0.22.2140(0x28001633) WeChat/arm64 Weixin NetType/WIFI Language/zh_CN ABI/arm64'
            headers['Accept'] = 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.9'
        
        response = requests.get(url, headers=headers, timeout=10)
        response.raise_for_status()
        
        soup = BeautifulSoup(response.text, 'html.parser')
        
        # 移除脚本和样式
        for script in soup(["script", "style"]):
            script.extract()
            
        text = soup.get_text(separator='\n')
        # 简单清洗空行
        lines = (line.strip() for line in text.splitlines())
        chunks = (phrase.strip() for line in lines for phrase in line.split("  "))
        text = '\n'.join(chunk for chunk in chunks if chunk)
        
        # 截取前 10000 字符，防止过长
        return text[:10000]
        
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
