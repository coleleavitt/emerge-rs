// archive.rs - Native archive extraction for all formats
//
// Supports: .tar.gz, .tar.bz2, .tar.xz, .tar.zst, .zip, .deb, .rar, .7z

use std::path::Path;
use std::fs::File;
use std::process::Command;
use crate::exception::InvalidData;

/// Archive format detection and extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveFormat {
    TarGz,
    TarBz2,
    TarXz,
    TarZst,
    Tar,
    Zip,
    Deb,
    Rar,
    SevenZip,
    Unknown,
}

impl ArchiveFormat {
    /// Detect format from filename
    pub fn detect(filename: &str) -> Self {
        let lower = filename.to_lowercase();
        
        if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
            Self::TarGz
        } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") || lower.ends_with(".tbz") {
            Self::TarBz2
        } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
            Self::TarXz
        } else if lower.ends_with(".tar.zst") || lower.ends_with(".tzst") {
            Self::TarZst
        } else if lower.ends_with(".tar") {
            Self::Tar
        } else if lower.ends_with(".zip") {
            Self::Zip
        } else if lower.ends_with(".deb") {
            Self::Deb
        } else if lower.ends_with(".rar") {
            Self::Rar
        } else if lower.ends_with(".7z") {
            Self::SevenZip
        } else {
            Self::Unknown
        }
    }
}

/// Extract archive to destination directory
pub fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let filename = archive_path.file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| InvalidData::new("Invalid archive filename", None))?;
    
    let format = ArchiveFormat::detect(filename);
    
    match format {
        ArchiveFormat::TarGz => extract_tar_gz(archive_path, dest_dir),
        ArchiveFormat::TarBz2 => extract_tar_bz2(archive_path, dest_dir),
        ArchiveFormat::TarXz => extract_tar_xz(archive_path, dest_dir),
        ArchiveFormat::TarZst => extract_tar_zst(archive_path, dest_dir),
        ArchiveFormat::Tar => extract_tar(archive_path, dest_dir),
        ArchiveFormat::Zip => extract_zip(archive_path, dest_dir),
        ArchiveFormat::Deb => extract_deb(archive_path, dest_dir),
        ArchiveFormat::Rar => extract_rar(archive_path, dest_dir),
        ArchiveFormat::SevenZip => extract_7z(archive_path, dest_dir),
        ArchiveFormat::Unknown => {
            Err(InvalidData::new(&format!("Unknown archive format: {}", filename), None))
        }
    }
}

/// Extract .tar.gz archive using tar crate
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open archive: {}", e), None))?;
    
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    
    archive.unpack(dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to extract tar.gz: {}", e), None))
}

/// Extract .tar.bz2 archive using tar crate
fn extract_tar_bz2(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open archive: {}", e), None))?;
    
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    
    archive.unpack(dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to extract tar.bz2: {}", e), None))
}

/// Extract .tar.xz archive using tar crate
fn extract_tar_xz(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open archive: {}", e), None))?;
    
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    
    archive.unpack(dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to extract tar.xz: {}", e), None))
}

/// Extract .tar.zst archive using zstd crate
fn extract_tar_zst(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open archive: {}", e), None))?;
    
    let decoder = zstd::Decoder::new(file)
        .map_err(|e| InvalidData::new(&format!("Failed to create zstd decoder: {}", e), None))?;
    
    let mut archive = tar::Archive::new(decoder);
    
    archive.unpack(dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to extract tar.zst: {}", e), None))
}

/// Extract plain .tar archive
fn extract_tar(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open archive: {}", e), None))?;
    
    let mut archive = tar::Archive::new(file);
    
    archive.unpack(dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to extract tar: {}", e), None))
}

/// Extract .zip archive using zip crate
fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let file = File::open(archive_path)
        .map_err(|e| InvalidData::new(&format!("Failed to open zip: {}", e), None))?;
    
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| InvalidData::new(&format!("Failed to read zip: {}", e), None))?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| InvalidData::new(&format!("Failed to read zip entry: {}", e), None))?;
        
        let outpath = match file.enclosed_name() {
            Some(path) => dest_dir.join(path),
            None => continue,
        };
        
        if file.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| InvalidData::new(&format!("Failed to create directory: {}", e), None))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| InvalidData::new(&format!("Failed to create parent directory: {}", e), None))?;
            }
            
            let mut outfile = File::create(&outpath)
                .map_err(|e| InvalidData::new(&format!("Failed to create file: {}", e), None))?;
            
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| InvalidData::new(&format!("Failed to write file: {}", e), None))?;
        }
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))
                    .map_err(|e| InvalidData::new(&format!("Failed to set permissions: {}", e), None))?;
            }
        }
    }
    
    Ok(())
}

/// Extract .deb archive (Debian package)
/// .deb files are ar archives containing control.tar.* and data.tar.*
fn extract_deb(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    // .deb files are ar archives
    // We'll use the ar command for now, but could use a pure Rust ar library
    
    // First, extract the ar archive to a temp directory
    let temp_dir = dest_dir.join(".deb_extract_tmp");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to create temp dir: {}", e), None))?;
    
    // Extract ar archive
    let status = Command::new("ar")
        .arg("x")
        .arg(archive_path)
        .current_dir(&temp_dir)
        .status()
        .map_err(|e| InvalidData::new(&format!("Failed to run ar: {}", e), None))?;
    
    if !status.success() {
        return Err(InvalidData::new("ar extraction failed", None));
    }
    
    // Find and extract data.tar.*
    let entries = std::fs::read_dir(&temp_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to read temp dir: {}", e), None))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        
        if filename_str.starts_with("data.tar") {
            // Extract this tar archive to the destination
            let data_tar = entry.path();
            extract_archive(&data_tar, dest_dir)?;
        }
    }
    
    // Clean up temp directory
    std::fs::remove_dir_all(&temp_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to remove temp dir: {}", e), None))?;
    
    Ok(())
}

/// Extract .rar archive using unrar command
fn extract_rar(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let status = Command::new("unrar")
        .arg("x")
        .arg("-o+")
        .arg(archive_path)
        .arg(dest_dir)
        .status()
        .map_err(|e| InvalidData::new(&format!("Failed to run unrar: {}", e), None))?;
    
    if status.success() {
        Ok(())
    } else {
        Err(InvalidData::new("unrar extraction failed", None))
    }
}

/// Extract .7z archive using 7z command
fn extract_7z(archive_path: &Path, dest_dir: &Path) -> Result<(), InvalidData> {
    let status = Command::new("7z")
        .arg("x")
        .arg(archive_path)
        .arg(format!("-o{}", dest_dir.display()))
        .status()
        .map_err(|e| InvalidData::new(&format!("Failed to run 7z: {}", e), None))?;
    
    if status.success() {
        Ok(())
    } else {
        Err(InvalidData::new("7z extraction failed", None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_format() {
        assert_eq!(ArchiveFormat::detect("archive.tar.gz"), ArchiveFormat::TarGz);
        assert_eq!(ArchiveFormat::detect("archive.tgz"), ArchiveFormat::TarGz);
        assert_eq!(ArchiveFormat::detect("archive.tar.bz2"), ArchiveFormat::TarBz2);
        assert_eq!(ArchiveFormat::detect("archive.tar.xz"), ArchiveFormat::TarXz);
        assert_eq!(ArchiveFormat::detect("archive.zip"), ArchiveFormat::Zip);
        assert_eq!(ArchiveFormat::detect("package.deb"), ArchiveFormat::Deb);
    }
}
