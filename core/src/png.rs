// Copyright (C) 2026 Synaelle
// SPDX-License-Identifier: GPL-3.0-or-later
//! BMP to PNG after a download: the 3DS saves its screenshots as BMP (Luma),
//! which most apps and websites don't take. Optional, in the settings and with
//! baguette-cli --png.

use std::path::Path;

/// is this a BMP, by its name
pub fn is_bmp(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case("bmp"))
}

/// the PNG next to it for a .bmp file
pub fn png_name(path: &str) -> String {
    Path::new(path).with_extension("png").to_string_lossy().into_owned()
}

/// converts a downloaded .bmp to a .png beside it and deletes the BMP.
/// Ok(None) for anything that isn't a .bmp; on an error the BMP stays.
pub fn convert(path: &str) -> Result<Option<String>, String> {
    if !is_bmp(path) {
        return Ok(None);
    }
    let img = image::open(path).map_err(|e| format!("can't read {path} as a BMP: {e}"))?;
    let png = png_name(path);
    img.save_with_format(&png, image::ImageFormat::Png).map_err(|e| {
        let _ = std::fs::remove_file(&png);
        format!("can't write {png}: {e}")
    })?;
    std::fs::remove_file(path).map_err(|e| format!("{png} written, but {path} stays: {e}"))?;
    Ok(Some(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_becomes_png() {
        let dir = std::env::temp_dir().join(format!("baguette-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bmp = dir.join("shot_top.BMP").to_string_lossy().into_owned();
        let mut img = image::RgbImage::new(400, 240);
        img.put_pixel(10, 20, image::Rgb([184, 40, 60]));
        img.save_with_format(&bmp, image::ImageFormat::Bmp).unwrap();

        let png = convert(&bmp).unwrap().unwrap();
        assert!(png.ends_with("shot_top.png"));
        assert!(!Path::new(&bmp).exists());
        let back = image::open(&png).unwrap().to_rgb8();
        assert_eq!((back.width(), back.height()), (400, 240));
        assert_eq!(back.get_pixel(10, 20), &image::Rgb([184, 40, 60]));

        let txt = dir.join("notes.txt").to_string_lossy().into_owned();
        std::fs::write(&txt, "hi").unwrap();
        assert_eq!(convert(&txt).unwrap(), None);
        // a broken BMP stays as it is
        let bad = dir.join("bad.bmp").to_string_lossy().into_owned();
        std::fs::write(&bad, "not a bmp").unwrap();
        assert!(convert(&bad).is_err() && Path::new(&bad).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
