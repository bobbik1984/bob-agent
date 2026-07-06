use rust_xlsxwriter::{Color, Format, Workbook};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct XlsxData {
    pub sheets: Vec<SheetData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SheetData {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<CellValue>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellValue {
    Number(f64),
    Text(String),
    Empty,
}

pub fn generate_xlsx<P: AsRef<Path>>(
    path: P,
    data: &XlsxData,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();

    // Define styles
    let header_format = Format::new()
        .set_bold()
        .set_background_color(Color::Theme(4, 0)) // 灰色背景
        .set_border(rust_xlsxwriter::FormatBorder::Thin);

    let row_format = Format::new().set_border(rust_xlsxwriter::FormatBorder::Thin);

    let alt_row_format = Format::new()
        .set_background_color(Color::Theme(4, 1)) // 更浅的灰色
        .set_border(rust_xlsxwriter::FormatBorder::Thin);

    for sheet_data in &data.sheets {
        let worksheet = workbook.add_worksheet().set_name(&sheet_data.name)?;

        // Write headers
        for (col, header) in sheet_data.headers.iter().enumerate() {
            worksheet.write_string_with_format(0, col as u16, header, &header_format)?;
            // 简单的列宽自适应：根据表头长度设置一个最小宽度
            worksheet.set_column_width(col as u16, (header.chars().count() * 2 + 5) as f64)?;
        }

        // 冻结首行
        worksheet.set_freeze_panes(1, 0)?;

        // Write data
        for (row_idx, row) in sheet_data.rows.iter().enumerate() {
            let row_num = (row_idx + 1) as u32;
            let current_format = if row_idx % 2 == 1 {
                &alt_row_format
            } else {
                &row_format
            };

            for (col_idx, cell) in row.iter().enumerate() {
                let col_num = col_idx as u16;
                match cell {
                    CellValue::Text(text) => {
                        worksheet.write_string_with_format(
                            row_num,
                            col_num,
                            text,
                            current_format,
                        )?;
                    }
                    CellValue::Number(num) => {
                        worksheet.write_number_with_format(
                            row_num,
                            col_num,
                            *num,
                            current_format,
                        )?;
                    }
                    CellValue::Empty => {
                        worksheet.write_blank(row_num, col_num, current_format)?;
                    }
                }
            }
        }
    }

    workbook.save(path)?;
    Ok(())
}
