use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct PptxData {
    pub template: String,
    pub slides: Vec<SlideData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SlideData {
    pub r#type: String, // "cover", "content", "section", "summary", "blank"
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub content: Option<String>,      // 正文（支持多段，用 \n\n 分隔）
    pub bullets: Option<Vec<String>>, // 要点列表
}

/// 颜色主题配置
struct ThemeColors {
    bg: &'static str,       // 背景色 (RRGGBB)
    title: &'static str,    // 标题色
    text: &'static str,     // 正文色
    accent: &'static str,   // 强调色
    subtitle: &'static str, // 副标题色
}

fn get_theme(name: &str) -> ThemeColors {
    match name {
        "corporate-light" => ThemeColors {
            bg: "FFFFFF",
            title: "1B1B1B",
            text: "404040",
            accent: "2563EB",
            subtitle: "6B7280",
        },
        _ => ThemeColors {
            // "corporate-dark" 默认深色主题
            bg: "1A1A2E",
            title: "EAEAEA",
            text: "B0B0C0",
            accent: "4F8FE6",
            subtitle: "8888AA",
        },
    }
}

/// EMU (English Metric Units) 转换: 1 inch = 914400 EMU, 1 cm = 360000 EMU
const SLIDE_W: i64 = 12192000; // 标准 16:9 宽 (33.867cm)
const SLIDE_H: i64 = 6858000; // 标准 16:9 高 (19.05cm)

/// 生成一个合法的 .pptx 文件
pub fn generate_pptx(path: &Path, data: &PptxData) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let theme = get_theme(&data.template);
    let slide_count = data.slides.len().max(1);

    // ── [Content_Types].xml ──────────────────────────────
    zip.start_file("[Content_Types].xml", options)?;
    let mut ct_overrides = String::new();
    for i in 1..=slide_count {
        ct_overrides.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
            i
        ));
    }
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
{ct_overrides}
</Types>"#
    )?;

    // ── _rels/.rels ──────────────────────────────────────
    zip.start_file("_rels/.rels", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#
    )?;

    // ── ppt/_rels/presentation.xml.rels ──────────────────
    zip.start_file("ppt/_rels/presentation.xml.rels", options)?;
    let mut pres_rels = String::new();
    pres_rels.push_str(r#"<Relationship Id="rId100" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#);
    pres_rels.push_str(r#"<Relationship Id="rId101" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>"#);
    for i in 1..=slide_count {
        pres_rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{}.xml"/>"#,
            i, i
        ));
    }
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
{pres_rels}
</Relationships>"#
    )?;

    // ── ppt/presentation.xml ─────────────────────────────
    zip.start_file("ppt/presentation.xml", options)?;
    let mut slide_list = String::new();
    for i in 1..=slide_count {
        slide_list.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + i, i));
    }
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId100"/></p:sldMasterIdLst>
<p:sldIdLst>{slide_list}</p:sldIdLst>
<p:sldSz cx="{SLIDE_W}" cy="{SLIDE_H}"/>
<p:notesSz cx="{SLIDE_H}" cy="{SLIDE_W}"/>
</p:presentation>"#
    )?;

    // ── ppt/theme/theme1.xml (minimal) ───────────────────
    zip.start_file("ppt/theme/theme1.xml", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="BobTheme">
<a:themeElements>
<a:clrScheme name="Bob">
<a:dk1><a:srgbClr val="{title}"/></a:dk1>
<a:lt1><a:srgbClr val="{bg}"/></a:lt1>
<a:dk2><a:srgbClr val="{title}"/></a:dk2>
<a:lt2><a:srgbClr val="{bg}"/></a:lt2>
<a:accent1><a:srgbClr val="{accent}"/></a:accent1>
<a:accent2><a:srgbClr val="{accent}"/></a:accent2>
<a:accent3><a:srgbClr val="{accent}"/></a:accent3>
<a:accent4><a:srgbClr val="{accent}"/></a:accent4>
<a:accent5><a:srgbClr val="{accent}"/></a:accent5>
<a:accent6><a:srgbClr val="{accent}"/></a:accent6>
<a:hlink><a:srgbClr val="{accent}"/></a:hlink>
<a:folHlink><a:srgbClr val="{accent}"/></a:folHlink>
</a:clrScheme>
<a:fontScheme name="Bob"><a:majorFont><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:majorFont><a:minorFont><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Bob"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme>
</a:themeElements>
</a:theme>"#,
        bg = theme.bg,
        title = theme.title,
        accent = theme.accent
    )?;

    // ── Slide Master & Layout (minimal) ──────────────────
    zip.start_file("ppt/slideMasters/_rels/slideMaster1.xml.rels", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#
    )?;

    zip.start_file("ppt/slideMasters/slideMaster1.xml", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{bg}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
</p:sldMaster>"#,
        bg = theme.bg
    )?;

    zip.start_file("ppt/slideLayouts/_rels/slideLayout1.xml.rels", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#
    )?;

    zip.start_file("ppt/slideLayouts/slideLayout1.xml", options)?;
    write!(
        zip,
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank">
<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>
</p:sldLayout>"#
    )?;

    // ── 生成每一页 Slide ─────────────────────────────────
    for (i, slide) in data.slides.iter().enumerate() {
        let idx = i + 1;

        // slide rels
        zip.start_file(format!("ppt/slides/_rels/slide{}.xml.rels", idx), options)?;
        write!(
            zip,
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#
        )?;

        // slide content
        zip.start_file(format!("ppt/slides/slide{}.xml", idx), options)?;
        let slide_xml = build_slide_xml(slide, &theme);
        zip.write_all(slide_xml.as_bytes())?;
    }

    zip.finish()?;
    Ok(())
}

/// XML 特殊字符转义
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 构建单页 Slide 的 XML
fn build_slide_xml(slide: &SlideData, theme: &ThemeColors) -> String {
    let slide_type = slide.r#type.as_str();
    let title = slide.title.as_deref().unwrap_or("");
    let subtitle = slide.subtitle.as_deref().unwrap_or("");
    let content = slide.content.as_deref().unwrap_or("");

    let shapes = match slide_type {
        "cover" => build_cover_shapes(title, subtitle, theme),
        "section" => build_section_shapes(title, subtitle, theme),
        "summary" => build_content_shapes(title, content, &slide.bullets, theme, true),
        _ => build_content_shapes(title, content, &slide.bullets, theme, false),
    };

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
<p:cSld>
<p:bg><p:bgPr><a:solidFill><a:srgbClr val="{bg}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>
<p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr/>
{shapes}
</p:spTree>
</p:cSld>
</p:sld>"#,
        bg = theme.bg
    )
}

/// 封面页：居中大标题 + 副标题
fn build_cover_shapes(title: &str, subtitle: &str, theme: &ThemeColors) -> String {
    let mut s = String::new();

    // 左侧装饰竖条
    s.push_str(&format!(
        r#"<p:sp>
<p:nvSpPr><p:cNvPr id="10" name="AccentBar"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="457200" y="1600000"/><a:ext cx="91440" cy="2400000"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
<a:solidFill><a:srgbClr val="{accent}"/></a:solidFill>
<a:ln><a:noFill/></a:ln>
</p:spPr>
</p:sp>"#,
        accent = theme.accent
    ));

    // 标题
    s.push_str(&format!(r#"<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="914400" y="1600000"/><a:ext cx="10363200" cy="1400000"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="b"/>
<a:p><a:pPr algn="l"/><a:r><a:rPr lang="zh-CN" sz="4000" b="1" dirty="0"><a:solidFill><a:srgbClr val="{title_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{title}</a:t></a:r></a:p>
</p:txBody>
</p:sp>"#, title_clr = theme.title, title = xml_escape(title)));

    // 副标题
    if !subtitle.is_empty() {
        s.push_str(&format!(r#"<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Subtitle"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="914400" y="3100000"/><a:ext cx="10363200" cy="600000"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="t"/>
<a:p><a:pPr algn="l"/><a:r><a:rPr lang="zh-CN" sz="2000" dirty="0"><a:solidFill><a:srgbClr val="{sub_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{subtitle}</a:t></a:r></a:p>
</p:txBody>
</p:sp>"#, sub_clr = theme.subtitle, subtitle = xml_escape(subtitle)));
    }

    s
}

/// 章节分隔页：居中标题 + 可选副标题
fn build_section_shapes(title: &str, subtitle: &str, theme: &ThemeColors) -> String {
    let mut s = String::new();

    // 居中标题
    s.push_str(&format!(r#"<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="914400" y="2200000"/><a:ext cx="10363200" cy="1200000"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="ctr"/>
<a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="zh-CN" sz="3600" b="1" dirty="0"><a:solidFill><a:srgbClr val="{accent}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{title}</a:t></a:r></a:p>
</p:txBody>
</p:sp>"#, accent = theme.accent, title = xml_escape(title)));

    if !subtitle.is_empty() {
        s.push_str(&format!(r#"<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Subtitle"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="914400" y="3500000"/><a:ext cx="10363200" cy="600000"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="t"/>
<a:p><a:pPr algn="ctr"/><a:r><a:rPr lang="zh-CN" sz="1800" dirty="0"><a:solidFill><a:srgbClr val="{sub_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{subtitle}</a:t></a:r></a:p>
</p:txBody>
</p:sp>"#, sub_clr = theme.subtitle, subtitle = xml_escape(subtitle)));
    }

    s
}

/// 内容页：标题 + 正文/要点列表
fn build_content_shapes(
    title: &str,
    content: &str,
    bullets: &Option<Vec<String>>,
    theme: &ThemeColors,
    is_summary: bool,
) -> String {
    let mut s = String::new();

    // 顶部强调线
    let accent_y = 457200; // ~0.5 inch from top
    s.push_str(&format!(
        r#"<p:sp>
<p:nvSpPr><p:cNvPr id="10" name="TopLine"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="457200" y="{accent_y}"/><a:ext cx="11277600" cy="36576"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
<a:solidFill><a:srgbClr val="{accent}"/></a:solidFill>
<a:ln><a:noFill/></a:ln>
</p:spPr>
</p:sp>"#,
        accent = theme.accent
    ));

    // 标题
    let title_sz = if is_summary { "2800" } else { "2400" };
    s.push_str(&format!(r#"<p:sp>
<p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="457200" y="548640"/><a:ext cx="11277600" cy="731520"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="b"/>
<a:p><a:pPr algn="l"/><a:r><a:rPr lang="zh-CN" sz="{title_sz}" b="1" dirty="0"><a:solidFill><a:srgbClr val="{title_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{title}</a:t></a:r></a:p>
</p:txBody>
</p:sp>"#, title_clr = theme.title, title = xml_escape(title)));

    // 正文区域
    let body_top = 1371600; // ~1.5 inch
    let body_height = 5029200; // 剩余空间

    // 构建段落列表
    let mut paragraphs = String::new();
    if let Some(ref bullets_list) = bullets {
        for bullet in bullets_list {
            paragraphs.push_str(&format!(
                r#"<a:p><a:pPr marL="342900" indent="-342900"><a:buChar char="&#x2022;"/></a:pPr><a:r><a:rPr lang="zh-CN" sz="1600" dirty="0"><a:solidFill><a:srgbClr val="{text_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{text}</a:t></a:r></a:p>"#,
                text_clr = theme.text, text = xml_escape(bullet)
            ));
        }
    }
    if !content.is_empty() {
        for para in content.split("\n\n") {
            let trimmed = para.trim();
            if trimmed.is_empty() {
                continue;
            }
            paragraphs.push_str(&format!(
                r#"<a:p><a:r><a:rPr lang="zh-CN" sz="1600" dirty="0"><a:solidFill><a:srgbClr val="{text_clr}"/></a:solidFill><a:latin typeface="Segoe UI"/><a:ea typeface="Microsoft YaHei"/></a:rPr><a:t>{text}</a:t></a:r></a:p>"#,
                text_clr = theme.text, text = xml_escape(trimmed)
            ));
        }
    }
    if paragraphs.is_empty() {
        paragraphs = format!(r#"<a:p><a:endParaRPr lang="zh-CN"/></a:p>"#);
    }

    s.push_str(&format!(
        r#"<p:sp>
<p:nvSpPr><p:cNvPr id="3" name="Content"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr>
<p:spPr>
<a:xfrm><a:off x="457200" y="{body_top}"/><a:ext cx="11277600" cy="{body_height}"/></a:xfrm>
<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
</p:spPr>
<p:txBody>
<a:bodyPr anchor="t" lIns="91440" tIns="45720" rIns="91440" bIns="45720"/>
{paragraphs}
</p:txBody>
</p:sp>"#
    ));

    s
}
