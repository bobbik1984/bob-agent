import os
import base64
from io import BytesIO
from PIL import Image

SOURCE_ICON = r"src-tauri\icons\icon.png"
HTML_OUT = r"dist-release\icon_preview_accurate.html"

def load_info():
    img = Image.open(SOURCE_ICON).convert("RGBA")
    W_orig, H_orig = img.size
    alpha_bbox = img.split()[-1].getbbox()
    if not alpha_bbox:
        alpha_bbox = (0, 0, W_orig, H_orig)
        
    B_left, B_top, B_right, B_bottom = alpha_bbox
    W_bbox = B_right - B_left
    H_bbox = B_bottom - B_top
    D_bbox = max(W_bbox, H_bbox)
    
    # We will need these to calculate the SVG frame position in CSS
    return img.crop(alpha_bbox), W_orig, H_orig, B_left, B_top, D_bbox

def resize_logo(logo, max_dim):
    ratio = max_dim / max(logo.width, logo.height)
    new_w = int(logo.width * ratio)
    new_h = int(logo.height * ratio)
    return logo.resize((new_w, new_h), Image.Resampling.LANCZOS)

def generate_preview_data(logo, scale, W_orig, H_orig, B_left, B_top, D_bbox, canvas_size=300):
    logo_max = int(canvas_size * scale)
    resized = resize_logo(logo, logo_max)
    
    # Put it on a transparent canvas
    fg = Image.new("RGBA", (canvas_size, canvas_size), (0,0,0,0))
    offset_x = (canvas_size - resized.width) // 2
    offset_y = (canvas_size - resized.height) // 2
    fg.paste(resized, (offset_x, offset_y), resized)
    
    buffered = BytesIO()
    fg.save(buffered, format="PNG")
    img_str = base64.b64encode(buffered.getvalue()).decode("utf-8")
    b64 = f"data:image/png;base64,{img_str}"
    
    # Calculate original SVG frame position
    # S is the scaling factor applied to the bounding box
    S = logo_max / D_bbox
    
    # Original SVG size in scaled coordinates
    svg_w = W_orig * S
    svg_h = H_orig * S
    
    # Top-left of SVG frame in adaptive canvas coordinates
    svg_x = offset_x - (B_left * S)
    svg_y = offset_y - (B_top * S)
    
    return b64, svg_x, svg_y, svg_w, svg_h

def main():
    logo, W_orig, H_orig, B_left, B_top, D_bbox = load_info()
    
    data_100 = generate_preview_data(logo, 1.00, W_orig, H_orig, B_left, B_top, D_bbox)
    data_060 = generate_preview_data(logo, 0.60, W_orig, H_orig, B_left, B_top, D_bbox)
    data_050 = generate_preview_data(logo, 0.50, W_orig, H_orig, B_left, B_top, D_bbox)
    data_045 = generate_preview_data(logo, 0.45, W_orig, H_orig, B_left, B_top, D_bbox)
    
    html = f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <title>Android Adaptive Icon Accurate Preview</title>
    <style>
        body {{
            font-family: system-ui, -apple-system, sans-serif;
            background: #f0f2f5;
            display: flex;
            flex-direction: column;
            align-items: center;
            padding: 40px;
        }}
        h1 {{ color: #333; margin-bottom: 10px; }}
        p.desc {{
            color: #666; margin-bottom: 40px; max-width: 800px;
            text-align: center; line-height: 1.6;
        }}
        .container {{
            display: flex; gap: 40px; flex-wrap: wrap; justify-content: center;
            margin-top: 50px;
        }}
        .canvas-box {{
            position: relative; width: 300px; height: 300px;
            background: rgba(0, 0, 0, 0.02);
            margin-bottom: 15px;
        }}
        .android-mask {{
            position: absolute; width: 200px; height: 200px;
            top: 50px; left: 50px; border-radius: 50%;
            background: white; box-shadow: 0 8px 16px rgba(0,0,0,0.1);
            overflow: hidden; border: 1px solid rgba(0,0,0,0.1);
            z-index: 1;
        }}
        .logo-layer {{
            position: absolute; width: 300px; height: 300px;
            top: -50px; left: -50px; /* Offset because it's inside the mask */
            z-index: 2;
        }}
        .logo-layer-faded {{
            position: absolute; width: 300px; height: 300px;
            top: 0; left: 0; 
            opacity: 0.25;
            z-index: 0;
            pointer-events: none;
        }}
        .svg-frame {{
            position: absolute; border: 2px dashed red;
            pointer-events: none; z-index: 10;
        }}
        .svg-frame::after {{
            content: "SVG Bounds (Illustrator 里的画板边缘)"; position: absolute;
            bottom: -25px; left: 0; color: red; font-size: 12px; font-weight: bold; width: 300px;
        }}
        .label {{
            text-align: center; font-size: 18px; font-weight: bold; color: #2776BB;
            margin-top: 50px;
        }}
        .note {{
            text-align: center; margin-top: 5px; font-size: 14px; color: #777;
        }}
    </style>
</head>
<body>

    <h1>[重新校准] 包含原 SVG 边缘映射的裁切模拟器</h1>
    <p class="desc">
        红色虚线框 = 您的 SVG 在 Illustrator 里的实际画布轮廓（被脚本缩放后映射在生成器里的位置）。<br>
        实线白圆 = 安卓安全裁切区 (占最终画布 66%)<br>
        您可以清楚看到：由于脚本扒掉了红框到蓝字之间的 Padding，所以蓝字变大了。比例越小，红框越小。
    </p>

    <div class="container">
        
        <div style="position: relative;">
            <div class="canvas-box">
                <div class="svg-frame" style="left: {data_100[1]}px; top: {data_100[2]}px; width: {data_100[3]}px; height: {data_100[4]}px;"></div>
                <img src="{data_100[0]}" class="logo-layer-faded">
                <div class="android-mask">
                    <img src="{data_100[0]}" class="logo-layer">
                </div>
            </div>
            <div class="label">LOGO_SCALE = 1.00</div>
            <div class="note">完全占满 108 画布 (惨遭屠戮)</div>
        </div>

        <div style="position: relative;">
            <div class="canvas-box">
                <!-- Data for 0.60 -->
                <div class="svg-frame" style="left: {data_060[1]}px; top: {data_060[2]}px; width: {data_060[3]}px; height: {data_060[4]}px;"></div>
                <img src="{data_060[0]}" class="logo-layer-faded">
                <div class="android-mask">
                    <img src="{data_060[0]}" class="logo-layer">
                </div>
            </div>
            <div class="label">LOGO_SCALE = 0.60</div>
            <div class="note">边缘被切</div>
        </div>

        <div style="position: relative;">
            <div class="canvas-box">
                <div class="svg-frame" style="left: {data_050[1]}px; top: {data_050[2]}px; width: {data_050[3]}px; height: {data_050[4]}px;"></div>
                <img src="{data_050[0]}" class="logo-layer-faded">
                <div class="android-mask">
                    <img src="{data_050[0]}" class="logo-layer">
                </div>
            </div>
            <div class="label">LOGO_SCALE = 0.50</div>
            <div class="note">安全</div>
        </div>

        <div style="position: relative;">
            <div class="canvas-box">
                <div class="svg-frame" style="left: {data_045[1]}px; top: {data_045[2]}px; width: {data_045[3]}px; height: {data_045[4]}px;"></div>
                <img src="{data_045[0]}" class="logo-layer-faded">
                <div class="android-mask">
                    <img src="{data_045[0]}" class="logo-layer">
                </div>
            </div>
            <div class="label">LOGO_SCALE = 0.45</div>
            <div class="note">精致</div>
        </div>

    </div>

</body>
</html>"""
    
    with open(HTML_OUT, "w", encoding="utf-8") as f:
        f.write(html)
        
    print(f"Generated {HTML_OUT}")

if __name__ == "__main__":
    main()
