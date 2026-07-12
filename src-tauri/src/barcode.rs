use serde::Serialize;
use rxing;
use rxing::ResultPoint;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct BarcodeResult {
    pub data: String,
    pub format: String,
    pub points: Vec<(f32, f32)>,
    pub bcbp_info: Option<BoardingPassInfo>,
}

#[derive(Serialize)]
pub struct BoardingPassInfo {
    pub passenger_name: String,
    pub pnr: String,
    pub origin: String,
    pub destination: String,
    pub carrier: String,
    pub flight_number: String,
    pub date: String,
    pub seat: String,
}

use base64::Engine;

pub fn decode_barcode_from_base64(base64_str: &str) -> Result<BarcodeResult, String> {
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(base64_str).map_err(|e| format!("Base64 decode error: {}", e))?;
    let img = image::load_from_memory(&bytes).map_err(|e| format!("Failed to load image from memory: {}", e))?;
    let img_luma = img.into_luma8();
    let width = img_luma.width();
    let height = img_luma.height();
    let raw_pixels = img_luma.into_raw();
    
    // 策略 1: 标准模式 + TryHarder
    let mut hints = rxing::DecodeHints::default();
    hints.TryHarder = Some(true);
    if let Ok(result) = rxing::helpers::detect_in_luma_with_hints(
        raw_pixels.clone(),
        width,
        height,
        None,
        &mut hints
    ) {
        return build_barcode_result(result);
    }
    
    // 策略 2: 带图像预处理滤镜（对截图/拍照更友好）
    let mut hints2 = rxing::DecodeHints::default();
    hints2.TryHarder = Some(true);
    if let Ok(result) = rxing::helpers::detect_in_luma_filtered_with_hints(
        raw_pixels,
        width,
        height,
        None,
        &mut hints2
    ) {
        return build_barcode_result(result);
    }
    
    Err("Barcode decode failed: NotFoundException (tried normal + filtered)".to_string())
}

fn build_barcode_result(result: rxing::RXingResult) -> Result<BarcodeResult, String> {
    let format_str = format!("{:?}", result.getBarcodeFormat()).to_lowercase();
    let data = result.getText().to_string();
    
    let mut bcbp_info = None;
    if format_str == "pdf_417" || format_str == "aztec" || format_str == "qr_code" {
        bcbp_info = parse_bcbp(&data);
    }
    
    let points = result.getRXingResultPoints().iter().map(|p| (p.x, p.y)).collect();
    
    Ok(BarcodeResult {
        data,
        format: format_str,
        points,
        bcbp_info,
    })
}

pub fn decode_barcode_from_image(image_path: &str) -> Result<BarcodeResult, String> {
    let img = image::open(image_path).map_err(|e| format!("Failed to open image: {}", e))?;
    let img_luma = img.into_luma8();
    let width = img_luma.width();
    let height = img_luma.height();
    let mut hints = rxing::DecodeHints::default();
    hints.TryHarder = Some(true);
    let result = rxing::helpers::detect_in_luma_with_hints(
        img_luma.into_raw(),
        width,
        height,
        None,
        &mut hints
    ).map_err(|e| format!("Barcode decode failed: {}", e))?;
    
    let format_str = format!("{:?}", result.getBarcodeFormat()).to_lowercase();
    let data = result.getText().to_string();
    
    let mut bcbp_info = None;
    if format_str == "pdf_417" || format_str == "aztec" || format_str == "qr_code" {
        bcbp_info = parse_bcbp(&data);
    }
    
    let points = result.getRXingResultPoints().iter().map(|p| (p.x, p.y)).collect();
    
    Ok(BarcodeResult {
        data,
        format: format_str,
        points,
        bcbp_info,
    })
}

pub fn parse_bcbp(raw: &str) -> Option<BoardingPassInfo> {
    if raw.len() < 58 { return None; }
    if !raw.starts_with('M') { return None; }
    
    // BCBP standard offsets
    let passenger_name = raw.chars().skip(2).take(20).collect::<String>().trim().to_string();
    let pnr = raw.chars().skip(23).take(7).collect::<String>().trim().to_string();
    let origin = raw.chars().skip(30).take(3).collect::<String>().trim().to_string();
    let destination = raw.chars().skip(33).take(3).collect::<String>().trim().to_string();
    let carrier = raw.chars().skip(36).take(3).collect::<String>().trim().to_string();
    let flight_number = raw.chars().skip(39).take(5).collect::<String>().trim().to_string();
    let date = raw.chars().skip(44).take(3).collect::<String>().trim().to_string();
    let seat = raw.chars().skip(48).take(4).collect::<String>().trim().to_string();
    
    Some(BoardingPassInfo {
        passenger_name,
        pnr,
        origin,
        destination,
        carrier,
        flight_number,
        date,
        seat,
    })
}

#[tauri::command]
pub fn system_decode_barcode(image_path: String) -> Result<serde_json::Value, String> {
    match decode_barcode_from_image(&image_path) {
        Ok(res) => Ok(serde_json::to_value(res).map_err(|e| format!("Serialization error: {}", e))?),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub fn system_decode_barcode_base64(base64_str: String) -> Result<serde_json::Value, String> {
    match decode_barcode_from_base64(&base64_str) {
        Ok(res) => Ok(serde_json::to_value(res).map_err(|e| format!("Serialization error: {}", e))?),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub fn system_parse_bcbp(raw: String) -> Result<Option<BoardingPassInfo>, String> {
    Ok(parse_bcbp(&raw))
}
