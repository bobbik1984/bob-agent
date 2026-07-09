use std::io::{Read, Write, Cursor};
use std::path::Path;
use zip::write::SimpleFileOptions;

pub fn pack_skills(skills_dir: &Path) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buffer));
        // Use SimpleFileOptions for zip 2.x
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        
        let mut dirs = vec![skills_dir.to_path_buf()];
        while let Some(dir) = dirs.pop() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    } else {
                        if let Ok(rel_path) = path.strip_prefix(skills_dir) {
                            let _ = zip.start_file(rel_path.to_string_lossy().replace("\\", "/"), options.clone());
                            if let Ok(mut f) = std::fs::File::open(&path) {
                                let mut content = Vec::new();
                                let _ = f.read_to_end(&mut content);
                                let _ = zip.write_all(&content);
                            }
                        }
                    }
                }
            }
        }
        let _ = zip.finish().map_err(|e| e.to_string())?;
    }
    Ok(buffer)
}

pub fn unpack_skills(data: &[u8], target_dir: &Path) -> Result<(), String> {
    let cursor = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => continue,
        };
        
        if file.name().ends_with('/') {
            let _ = std::fs::create_dir_all(&outpath);
        } else {
            if let Some(p) = outpath.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            if let Ok(mut outfile) = std::fs::File::create(&outpath) {
                let _ = std::io::copy(&mut file, &mut outfile);
            }
        }
    }
    Ok(())
}
