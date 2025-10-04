// eapi.rs - EAPI (Ebuild API) version handling
//
// Different EAPI versions have different default phase behaviors

use crate::exception::InvalidData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eapi {
    Eapi5,
    Eapi6,
    Eapi7,
    Eapi8,
    Eapi9,
}

impl Eapi {
    /// Parse EAPI from string
    pub fn from_str(eapi: &str) -> Result<Self, InvalidData> {
        match eapi {
            "5" => Ok(Eapi::Eapi5),
            "6" => Ok(Eapi::Eapi6),
            "7" => Ok(Eapi::Eapi7),
            "8" => Ok(Eapi::Eapi8),
            "9" => Ok(Eapi::Eapi9),
            _ => Err(InvalidData::new(&format!("Unsupported EAPI: {}", eapi), None)),
        }
    }
    
    /// Get EAPI as string
    pub fn as_str(&self) -> &str {
        match self {
            Eapi::Eapi5 => "5",
            Eapi::Eapi6 => "6",
            Eapi::Eapi7 => "7",
            Eapi::Eapi8 => "8",
            Eapi::Eapi9 => "9",
        }
    }
    
    /// Does this EAPI support default_src_prepare?
    pub fn has_default_src_prepare(&self) -> bool {
        match self {
            Eapi::Eapi5 => false,
            _ => true, // EAPI 6+ has default_src_prepare
        }
    }
    
    /// Does this EAPI die on ECONF_SOURCE not existing?
    pub fn strict_econf_source(&self) -> bool {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 => false,
            _ => true,
        }
    }
    
    /// Does this EAPI require dosym arguments in specific order?
    pub fn strict_dosym(&self) -> bool {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 | Eapi::Eapi7 => false,
            _ => true, // EAPI 8+ requires -r for relative symlinks
        }
    }
    
    /// Does this EAPI support ver_* functions natively?
    pub fn has_ver_functions(&self) -> bool {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 => false,
            _ => true, // EAPI 7+ has ver_cut, ver_rs, etc.
        }
    }
    
    /// Does this EAPI support BDEPEND?
    pub fn has_bdepend(&self) -> bool {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 => false,
            _ => true, // EAPI 7+ has BDEPEND
        }
    }
    
    /// Does this EAPI support IDEPEND?
    pub fn has_idepend(&self) -> bool {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 | Eapi::Eapi7 => false,
            _ => true, // EAPI 8+ has IDEPEND
        }
    }
    
    /// Get default src_configure arguments
    pub fn default_src_configure_args(&self) -> Vec<String> {
        match self {
            Eapi::Eapi5 => vec![
                "--prefix=/usr".to_string(),
                "--build=x86_64-pc-linux-gnu".to_string(),
                "--host=x86_64-pc-linux-gnu".to_string(),
                "--mandir=/usr/share/man".to_string(),
                "--infodir=/usr/share/info".to_string(),
                "--datadir=/usr/share".to_string(),
                "--sysconfdir=/etc".to_string(),
                "--localstatedir=/var/lib".to_string(),
            ],
            _ => vec![
                "--prefix=/usr".to_string(),
                "--sysconfdir=/etc".to_string(),
                "--localstatedir=/var".to_string(),
            ],
        }
    }
    
    /// Get install directory for documentation
    pub fn doc_install_dir(&self, pf: &str) -> String {
        match self {
            Eapi::Eapi5 | Eapi::Eapi6 => format!("/usr/share/doc/{}", pf),
            _ => format!("/usr/share/doc/{}/html", pf),
        }
    }
    
    /// Should we call default for this phase if no custom implementation?
    pub fn has_default_phase(&self, phase: &str) -> bool {
        match phase {
            "src_prepare" => self.has_default_src_prepare(),
            "src_configure" | "src_compile" | "src_install" => true,
            _ => false,
        }
    }
}

impl Default for Eapi {
    fn default() -> Self {
        Eapi::Eapi8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_eapi_parsing() {
        assert_eq!(Eapi::from_str("7").unwrap(), Eapi::Eapi7);
        assert_eq!(Eapi::from_str("8").unwrap(), Eapi::Eapi8);
        assert!(Eapi::from_str("10").is_err());
    }
    
    #[test]
    fn test_eapi_features() {
        let eapi5 = Eapi::Eapi5;
        let eapi8 = Eapi::Eapi8;
        
        assert!(!eapi5.has_default_src_prepare());
        assert!(eapi8.has_default_src_prepare());
        
        assert!(!eapi5.has_ver_functions());
        assert!(eapi8.has_ver_functions());
        
        assert!(!eapi5.has_bdepend());
        assert!(eapi8.has_bdepend());
    }
}
