use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{ImageBuffer, Rgba};
use pdfium_render::prelude::*;
use std::path::Path;

pub fn render_pdf_to_images(path: &str, max_pages: u16) -> Result<Vec<String>, String> {
    let bind = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|e| format!("无法绑定 PDFium 引擎: {}", e))?;

    let pdfium = Pdfium::new(bind);
    let document = pdfium
        .load_pdf_from_file(Path::new(path), None)
        .map_err(|e| format!("PDFium 加载文件失败: {}", e))?;

    let mut images_base64 = Vec::new();
    let pages = document.pages();
    let num_pages = pages.len();

    // limit max_pages to avoid OOM
    let pages_to_render = std::cmp::min(num_pages, max_pages);

    for i in 0..pages_to_render {
        let page = pages
            .get(i)
            .map_err(|e| format!("无法获取第 {} 页: {}", i, e))?;

        let render_config = PdfRenderConfig::new()
            .set_target_width(1200) // render width
            .set_clear_color(PdfColor::new(255, 255, 255, 255)); // white background

        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| format!("页面 {} 渲染失败: {}", i, e))?;

        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;

        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        for pixel in bitmap.as_bytes().chunks_exact(4) {
            rgba_data.push(pixel[2]); // R (BGRA -> RGBA)
            rgba_data.push(pixel[1]); // G
            rgba_data.push(pixel[0]); // B
            rgba_data.push(pixel[3]); // A
        }

        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, rgba_data).ok_or("无法构建图像缓冲区")?;

        let mut png_bytes: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        img.write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| format!("图像编码为PNG失败: {}", e))?;

        let base64_str = STANDARD.encode(&png_bytes);
        images_base64.push(base64_str);
    }

    Ok(images_base64)
}
