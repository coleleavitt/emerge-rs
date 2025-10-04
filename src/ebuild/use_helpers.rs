// use_helpers.rs - USE flag helper functions
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;

/// Check if USE flag is enabled
pub fn use_enable(env: &EbuildEnvironment, flag: &str) -> bool {
    env.use_flag_enabled(flag)
}

/// Use flag with exec (returns value for command substitution)
pub fn usex(env: &EbuildEnvironment, flag: &str, enabled_val: &str, disabled_val: &str, enabled_suffix: Option<&str>, disabled_suffix: Option<&str>) -> String {
    let base = if use_enable(env, flag) {
        enabled_val
    } else {
        disabled_val
    };
    
    let suffix = if use_enable(env, flag) {
        enabled_suffix.unwrap_or("")
    } else {
        disabled_suffix.unwrap_or("")
    };
    
    format!("{}{}", base, suffix)
}

/// Use with flag - returns --with-flag or --without-flag for configure
pub fn use_with(env: &EbuildEnvironment, flag: &str, opt: Option<&str>, value: Option<&str>) -> String {
    let option = opt.unwrap_or(flag);
    
    if use_enable(env, flag) {
        if let Some(val) = value {
            format!("--with-{}={}", option, val)
        } else {
            format!("--with-{}", option)
        }
    } else {
        format!("--without-{}", option)
    }
}

/// Use enable flag - returns --enable-flag or --disable-flag for configure
pub fn use_enable_flag(env: &EbuildEnvironment, flag: &str, opt: Option<&str>, value: Option<&str>) -> String {
    let option = opt.unwrap_or(flag);
    
    if use_enable(env, flag) {
        if let Some(val) = value {
            format!("--enable-{}={}", option, val)
        } else {
            format!("--enable-{}", option)
        }
    } else {
        format!("--disable-{}", option)
    }
}

/// In IUSE check - check if flag is in IUSE
pub fn in_iuse(env: &EbuildEnvironment, flag: &str) -> bool {
    if let Some(iuse) = env.get("IUSE") {
        iuse.split_whitespace()
            .any(|f| f.trim_start_matches(&['+', '-'][..]) == flag)
    } else {
        false
    }
}

/// Use if in IUSE - only check USE if flag is in IUSE
pub fn use_if_iuse(env: &EbuildEnvironment, flag: &str) -> bool {
    in_iuse(env, flag) && use_enable(env, flag)
}

/// CMake use - for CMake boolean options
pub fn cmake_use(env: &EbuildEnvironment, flag: &str, option: Option<&str>) -> String {
    let opt = option.unwrap_or(flag).to_uppercase().replace('-', "_");
    
    if use_enable(env, flag) {
        format!("-D{}=ON", opt)
    } else {
        format!("-D{}=OFF", opt)
    }
}

/// CMake use find package
pub fn cmake_use_find_package(env: &EbuildEnvironment, flag: &str, package: Option<&str>) -> String {
    let pkg = package.unwrap_or(flag);
    
    if use_enable(env, flag) {
        format!("-DCMAKE_DISABLE_FIND_PACKAGE_{}=OFF", pkg)
    } else {
        format!("-DCMAKE_DISABLE_FIND_PACKAGE_{}=ON", pkg)
    }
}

/// Meson use - for Meson options
pub fn meson_use(env: &EbuildEnvironment, flag: &str, option: Option<&str>) -> String {
    let opt = option.unwrap_or(flag);
    
    if use_enable(env, flag) {
        format!("-D{}=enabled", opt)
    } else {
        format!("-D{}=disabled", opt)
    }
}

/// Meson feature - for Meson feature options
pub fn meson_feature(env: &EbuildEnvironment, flag: &str, option: Option<&str>) -> String {
    meson_use(env, flag, option)
}

/// Qt feature - for Qt configure options  
pub fn qt_feature(env: &EbuildEnvironment, flag: &str, option: Option<&str>) -> String {
    let opt = option.unwrap_or(flag);
    
    if use_enable(env, flag) {
        format!("-feature-{}", opt)
    } else {
        format!("-no-feature-{}", opt)
    }
}

/// Qt use - for Qt configure options
pub fn qt_use(env: &EbuildEnvironment, flag: &str, option: Option<&str>) -> String {
    let opt = option.unwrap_or(flag);
    
    if use_enable(env, flag) {
        format!("-{}", opt)
    } else {
        format!("-no-{}", opt)
    }
}
